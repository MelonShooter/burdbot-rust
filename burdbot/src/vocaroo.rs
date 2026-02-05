use bytes::Bytes;
use futures::future;
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::Client;
use std::iter;
use std::num::ParseIntError;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use thiserror::Error;

pub type Result<'a, T> = std::result::Result<T, VocarooError<'a>>;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum VocarooError<'a> {
    #[error("No vocaroo URLs were found from the link: {0}.")]
    InvalidUrls(&'a str),
    #[error(
        "Failed Vocaroo HEAD request from the link: {0}. This could mean they stopped accepting these requests. Encountered reqwest error: {1}"
    )]
    FailedHead(String, #[source] reqwest::Error),
    #[error(
        "Failed Vocaroo GET request from the link: {0}. This could mean this isn't the right URL anymore. Encountered reqwest error: {1}"
    )]
    FailedGet(String, #[source] reqwest::Error),
    #[error("Failed to get an MP3 from the link: {0}. No content type was provided.")]
    NoContentType(String),
    #[error(
        "Failed to get an MP3 from the link: {0}. The content type header wasn't visible ASCII."
    )]
    ContentTypeNotVisibleASCII(String),
    #[error(
        "Failed to get an MP3 from the link: {0}. The content type wasn't that of an MP3, probably because the vocaroo ID was incorrect."
    )]
    ContentTypeNotMp3(String),
    #[error(
        "Failed to download vocaroo recording from https://media.vocaroo.com for link: {0}. Status {1} given. If status 404 was given, this probably just means the vocaroo recording expired. If the recording hasn't expired, check they're not using https://media1.vocaroo.com to host it. The JS code currently doesn't actually match which CDN server to use but it appears all new recordings are at https://media.vocaroo.com, not https://media1.vocaroo.com when this code was last touched. Check the current code by looking up usages of 'mediaMP3FileURL' on the site's JS code in the script at the bottom of the body."
    )]
    FailedDownload(String, u16),
    #[error(
        "Vocaroo didn't send the content length header in the HEAD request from the link: {0}."
    )]
    NoContentLength(String),
    #[error(
        "Failed to convert the provided content length header in the HEAD request from the link: {0} because it contained non-visible ASCII."
    )]
    ContentLengthNotVisibleASCII(String),
    #[error(
        "Failed to convert the provided visible ASCII content length header in the HEAD request from the link: {0} because it wasn't a number. Error encountered: {1}"
    )]
    ContentLengthNotNumber(String, #[source] ParseIntError),
    #[error(
        "Could not convert response body to bytes from the link: {0}. Encountered reqwest error: {1}"
    )]
    BodyToBytesFailure(String, #[source] reqwest::Error),
    #[error(
        "Vocaroo file at link '{0}' couldn't be converted to an MP3 because it would go over the total size limit: {1}."
    )]
    OverSizeLimit(String, usize),
}

fn get_vocaroo_mp3_urls<'a>(url_str: &'a str) -> impl Iterator<Item = String> + 'a {
    lazy_static! {
        static ref VOCAROO_LINK_MATCHER: Regex =
            Regex::new(r"https?://(?:www\.)?(?:voca\.ro|vocaroo\.com)/([a-zA-Z0-9]+)").unwrap();
    }

    VOCAROO_LINK_MATCHER
        .captures_iter(url_str)
        .flat_map(|c| c.get(1))
        .map(|vocaroo_id| format!("https://media.vocaroo.com/mp3/{}", vocaroo_id.as_str()))
}

pub async fn get_content_length(url: &str, client: Client) -> Result<'static, usize> {
    let head_response = client
        .head(url)
        .send()
        .await
        .map_err(|err| VocarooError::FailedHead(url.to_string(), err))?;
    let headers = head_response.headers();
    let content_type = headers
        .get("Content-Type")
        .ok_or_else(|| VocarooError::NoContentType(url.to_string()))?
        .to_str()
        .map_err(|_| VocarooError::ContentTypeNotVisibleASCII(url.to_string()))?;

    if content_type != "audio/mpeg" {
        return Err(VocarooError::ContentTypeNotMp3(url.to_string()));
    }

    headers
        .get("Content-Length")
        .ok_or_else(|| VocarooError::NoContentLength(url.to_string()))?
        .to_str()
        .map_err(|_| VocarooError::ContentLengthNotVisibleASCII(url.to_string()))?
        .parse::<usize>()
        .map_err(|err| VocarooError::ContentLengthNotNumber(url.to_string(), err))
}

pub async fn download_vocaroos(
    urls: &str, max_size: usize, attachment_count_limit: usize,
) -> impl Iterator<Item = Result<'_, Bytes>> {
    lazy_static! {
        static ref VOCAROO_CLIENT: Client = Client::new();
    }

    let recordings = {
        let total_recording_size = Arc::new(AtomicUsize::new(0));
        let vocaroo_urls =
            get_vocaroo_mp3_urls(urls).zip(iter::repeat(total_recording_size.clone()));

        let recording_futures = vocaroo_urls.map(|(url, total_size)| async move {
            let content_length =
                get_content_length(url.as_str(), (*VOCAROO_CLIENT).clone()).await?;
            let total_size =
                total_size.fetch_add(content_length, Ordering::Relaxed) + content_length;

            if total_size > max_size {
                return Err(VocarooError::OverSizeLimit(url, max_size));
            }

            let response = VOCAROO_CLIENT
                .get(&*url)
                .send()
                .await
                .map_err(|err| VocarooError::FailedGet(url.clone(), err))?;

            if !response.status().is_success() {
                return Err(VocarooError::FailedDownload(url, response.status().as_u16()));
            }

            response.bytes().await.map_err(|err| VocarooError::BodyToBytesFailure(url, err))
        });

        future::join_all(recording_futures).await
    };

    recordings.into_iter().take(attachment_count_limit)
}
