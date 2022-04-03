use std::num::ParseIntError;

use bytes::Bytes;
use lazy_static::lazy_static;
use log::error;
use regex::Regex;
use reqwest::Client;
use thiserror::Error;

pub type Result<'a, T> = std::result::Result<T, VocarooError<'a>>;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum VocarooError<'a> {
    #[error("Malformed vocaroo URL provided from the link: {0}.")]
    MalformedUrl(&'a str),
    #[error("Failed Vocaroo HEAD request from the link: {0}. This could mean they stopped accepting these requests. Encountered reqwest error: {1}")]
    FailedHead(String, #[source] reqwest::Error),
    #[error("Failed Vocaroo GET request from the link: {0}. This could mean this isn't the right URL anymore. Encountered reqwest error: {1}")]
    FailedGet(String, #[source] reqwest::Error),
    #[error("Failed to get an MP3 from the link: {0}. No content type was provided.")]
    NoContentType(String),
    #[error("Failed to get an MP3 from the link: {0}. The content type header wasn't visible ASCII.")]
    ContentTypeNotVisibleASCII(String),
    #[error("Failed to get an MP3 from the link: {0}. The content type wasn't that of an MP3, probably because the vocaroo ID was incorrect.")]
    ContentTypeNotMp3(String),
    #[error("Failed to download vocaroo recording from https://media.vocaroo.com for link: {0}. Status {1} given. If status 404 was given, this probably just means the vocaroo recording expired. If the recording hasn't expired, check they're not using https://media1.vocaroo.com to host it. The JS code currently doesn't actually match which CDN server to use but it appears all new recordings are at https://media.vocaroo.com, not https://media1.vocaroo.com when this code was last touched. Check the current code by looking up usages of 'mediaMP3FileURL' on the site's JS code in the script at the bottom of the body.")]
    FailedDownload(String, u16),
    #[error("Vocaroo didn't send the content length header in the HEAD request from the link: {0}.")]
    NoContentLength(String),
    #[error("Failed to convert the provided content length header in the HEAD request from the link: {0} because it contained non-visible ASCII.")]
    ContentLengthNotVisibleASCII(String),
    #[error("Failed to convert the provided visible ASCII content length header in the HEAD request from the link: {0} because it wasn't a number. Error encountered: {1}")]
    ContentLengthNotNumber(String, #[source] ParseIntError),
    #[error("Could not convert response body to bytes from the link: {0}. Encountered reqwest error: {1}")]
    BodyToBytesFailure(String, #[source] reqwest::Error),
    #[error("Vocaroo file at link '{0}' couldn't be converted to an MP3 because it was over the size limit: {1}.")]
    OversizedFile(String, u64),
}

fn get_vocaroo_mp3_url(url: &str) -> Result<String> {
    lazy_static! {
        static ref VOCAROO_LINK_MATCHER: Regex = Regex::new(r"https?://(?:www\.)?(?:voca\.ro|vocaroo\.com)/([a-zA-Z0-9]+)").unwrap();
    }

    let vocaroo_id = match VOCAROO_LINK_MATCHER.captures(url).map(|c| c.get(1)) {
        Some(Some(m)) => m.as_str(),
        Some(None) => {
            error!("Error encountered matching vocaroo ID in link. This should never happen. Returning MalformedUrl error.");

            return Err(VocarooError::MalformedUrl(url));
        }
        None => return Err(VocarooError::MalformedUrl(url)),
    };

    Ok(format!("https://media.vocaroo.com/mp3/{vocaroo_id}"))
}

pub async fn download_vocaroo<'url>(url: &'url str, max_size: u64) -> Result<'_, Bytes> {
    lazy_static! {
        static ref VOCAROO_CLIENT: Client = Client::new();
    }

    // Technically these string clones aren't necessary if I were to use match, but it would make the code way less readable. They'll probably get optimized out anyways though.
    let url: String = get_vocaroo_mp3_url(url)?;
    let head_response = VOCAROO_CLIENT
        .head(&*url)
        .send()
        .await
        .map_err(|err| VocarooError::FailedHead(url.clone(), err))?;
    let headers = head_response.headers();
    let content_type = headers
        .get("Content-Type")
        .ok_or_else(|| VocarooError::NoContentType(url.clone()))?
        .to_str()
        .map_err(|_| VocarooError::ContentTypeNotVisibleASCII(url.clone()))?;

    if content_type != "audio/mpeg" {
        return Err(VocarooError::ContentTypeNotMp3(url));
    }

    let content_length = headers
        .get("Content-Length")
        .ok_or_else(|| VocarooError::NoContentLength(url.clone()))?
        .to_str()
        .map_err(|_| VocarooError::ContentLengthNotVisibleASCII(url.clone()))?
        .parse::<u64>()
        .map_err(|err| VocarooError::ContentLengthNotNumber(url.clone(), err))?;

    if content_length > max_size {
        return Err(VocarooError::OversizedFile(url, max_size));
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
}
