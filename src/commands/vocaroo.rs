use std::collections::HashSet;
use std::num::ParseIntError;

use bytes::Bytes;
use lazy_static::lazy_static;
use log::{debug, error, warn};
use regex::Regex;
use reqwest::Client;
use reqwest::Error as ReqwestError;
use rusqlite::{Connection, Error as SqliteError};
use serenity::builder::CreateMessage;
use serenity::client::Context;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::CommandResult;
use serenity::model::channel::{Message, MessageReference};
use serenity::model::id::{ChannelId, GuildId};
use serenity::prelude::TypeMapKey;
use serenity::Error;
use thiserror::Error;

use crate::BURDBOT_DB;

use super::error_util::{dm_issue_no_return, IssueType};
use super::util;

const MAX_VOCAROO_RECORDING_SIZE: u32 = (1 << 20) * 5; // 5MB

struct VocarooEnabled;

impl TypeMapKey for VocarooEnabled {
    type Value = HashSet<u64>;
}

#[non_exhaustive]
#[derive(Debug, Error)]
enum VocarooError<'a> {
    #[error("Failed Vocaroo HEAD request while converting the link: {0}. This could mean they stopped accepting these requests. Encountered reqwest error: {1}")]
    FailedHead(&'a str, #[source] ReqwestError),
    #[error(
        "Failed Vocaroo GET request while converting the link: {0}. This could mean this isn't the right URL anymore. Encountered reqwest error: {1}"
    )]
    FailedGet(&'a str, #[source] ReqwestError),
    #[error("Failed to download vocaroo recording from https://media.vocaroo.com for link: {0}. Status {1} given. If status 404 was given, this probably just means the vocaroo recording expired. If the recording hasn't expired, check they're not using https://media1.vocaroo.com to host it. The JS code currently doesn't actually match which CDN server to use but it appears all new recordings are at https://media.vocaroo.com, not https://media1.vocaroo.com when this code was last touched. Check the current code by looking up usages of 'mediaMP3FileURL' on the site's JS code in the script at the bottom of the body.")]
    FailedDownload(&'a str, u16),
    #[error("Vocaroo didn't send the content length header in the HEAD request while converting the link: {0}.")]
    NoContentLength(&'a str),
    #[error("Failed to convert the provided content length header in the HEAD request while converting the link: {0} because it wasn't made using visible ASCII.")]
    ContentLengthNotVisibleASCII(&'a str),
    #[error("Failed to convert the provided visible ASCII content length header in the HEAD request while converting the link: {0} because it wasn't a number. Error encountered: {1}")]
    ContentLengthNotNumber(&'a str, #[source] ParseIntError),
    #[error("Could not convert response body to bytes while trying to convert the link: {0}. Encountered reqwest error: {1}")]
    BodyToBytesFailure(&'a str, #[source] ReqwestError),
    #[error("Vocaroo file at link '{0}' couldn't be converted to an MP3 because it was over the size limit: {MAX_VOCAROO_RECORDING_SIZE}.")]
    OversizedFile(&'a str),
}

async fn download_vocaroo<'a>(client: &Client, url: &'a str) -> Result<Bytes, VocarooError<'a>> {
    let head_response = client.head(url).send().await.map_err(|err| VocarooError::FailedHead(url, err))?;
    let content_length_header = head_response
        .headers()
        .get("Content-Length")
        .ok_or_else(|| VocarooError::NoContentLength(url))?;

    let content_length = content_length_header
        .to_str()
        .map_err(|_| VocarooError::ContentLengthNotVisibleASCII(url))?
        .parse::<u32>()
        .map_err(|err| VocarooError::ContentLengthNotNumber(url, err))?;

    if content_length > MAX_VOCAROO_RECORDING_SIZE {
        return Err(VocarooError::OversizedFile(url));
    }

    let response = client.get(url).send().await.map_err(|err| VocarooError::FailedGet(url, err))?;

    if !response.status().is_success() {
        return Err(VocarooError::FailedDownload(url, response.status().as_u16()));
    }

    response.bytes().await.map_err(|err| VocarooError::BodyToBytesFailure(url, err))
}

async fn handle_vocaroo_error(ctx: &Context, msg: &Message, error: VocarooError<'_>) {
    let issue_type = match error {
        VocarooError::FailedDownload(_, _) => IssueType::Warning,
        VocarooError::OversizedFile(_) => IssueType::Debug,
        _ => IssueType::Error,
    };

    dm_issue_no_return::<(), VocarooError>(ctx, "vocaroo", &Err(error), "None.", issue_type).await;

    if let Err(err) = msg.react(&ctx.http, '❌').await {
        let link = msg.link();

        debug!("Failed to react to a vocaroo recording that errored. Error: {err}. Message link: {link}.");
    }
}

fn format_recording<'a, 'b>(
    msg_builder: &'b mut CreateMessage<'a>,
    vocaroo_data: &'a [u8],
    user_id: u64,
    msg_ref: MessageReference,
) -> &'b mut CreateMessage<'a> {
    msg_builder.add_file((vocaroo_data, "vocaroo-to-mp3.mp3"));

    let mut msg_str = String::with_capacity(96);
    msg_str.push_str("Here is <@");
    msg_str.push_str(user_id.to_string().as_str());
    msg_str.push_str(">'s vocaroo link as an MP3 file. This is limited to 1 per message.");

    msg_builder.content(msg_str.as_str());

    msg_builder.reference_message(msg_ref)
}

async fn send_recording(ctx: &Context, channel_id: ChannelId, guild_id: GuildId, vocaroo_data: Bytes, user_id: u64, msg_ref: MessageReference) {
    let msg_result = channel_id
        .send_message(&ctx.http, |c| format_recording(c, &vocaroo_data[..], user_id, msg_ref))
        .await;

    match msg_result {
        Ok(_) => (),
        Err(Error::Http(err)) => {
            debug!(
                "Couldn't send vocaroo message in channel {channel_id} in server {guild_id} because of HTTP error: {err:?}). \
                 This is generally due to a lack of permissions."
            )
        }
        Err(err) => warn!("Couldn't send vocaroo message in channel {channel_id} in server {guild_id} because of error: {err}"),
    };
}

pub async fn on_message_received(ctx: &Context, msg: &Message) {
    let data = ctx.data.read().await;
    let vocaroo_servers = data.get::<VocarooEnabled>();

    if let Some(servers) = vocaroo_servers {
        let guild_id = match msg.guild_id {
            Some(id) => id,
            None => return,
        };

        if servers.contains(&guild_id.0) {
            lazy_static! {
                static ref VOCAROO_LINK_MATCHER: Regex = Regex::new(r"https?://(?:www\.)?(?:voca\.ro|vocaroo\.com)/([a-zA-Z0-9]+)").unwrap();
                static ref VOCAROO_CLIENT: Client = Client::new();
            }

            let vocaroo_id;

            {
                let vocaroo_capture = match VOCAROO_LINK_MATCHER.captures(msg.content.as_str()) {
                    Some(capture) => capture,
                    None => return,
                };

                let id = vocaroo_capture.get(1).expect("Expected vocaroo ID to always exist").as_str();

                vocaroo_id = id.to_owned();
            }

            let msg_ref = MessageReference::from(msg);
            let channel_id = msg.channel_id;
            let user_id = msg.author.id.0;
            let vocaroo_url = format!("https://media.vocaroo.com/mp3/{vocaroo_id}");

            match download_vocaroo(&*VOCAROO_CLIENT, vocaroo_url.as_str()).await {
                Ok(vocaroo_data) => {
                    send_recording(ctx, channel_id, guild_id, vocaroo_data, user_id, msg_ref).await;
                }
                Err(error) => handle_vocaroo_error(ctx, msg, error).await,
            }
        }
    }
}

fn get_all_vocaroo_servers() -> Result<HashSet<u64>, SqliteError> {
    let mut vocaroo_servers = HashSet::new();

    let connection = Connection::open(BURDBOT_DB)?;

    let mut statement = connection.prepare(
        "
            SELECT guild_id FROM vocaroo_enabled;
        ",
    )?;

    let query = statement.query_map([], |row| row.get::<_, u64>(0))?;

    for guild_id in query {
        let id = guild_id?;

        vocaroo_servers.insert(id);
    }

    Ok(vocaroo_servers)
}

pub async fn on_ready(ctx: &Context) {
    let vocaroo_servers = get_all_vocaroo_servers();

    if let Ok(servers) = vocaroo_servers {
        let mut data = ctx.data.write().await;

        data.insert::<VocarooEnabled>(servers);
    }
}

#[command]
#[bucket("db_operations")]
async fn enablevocarootomp3(ctx: &Context, msg: &Message) -> CommandResult {
    let mut data = ctx.data.write().await;
    let vocaroo_servers = match data.get_mut::<VocarooEnabled>() {
        Some(servers) => servers,
        None => return Ok(()),
    };

    let guild_id = msg.guild_id.unwrap().0;

    if vocaroo_servers.insert(guild_id) {
        let connection = Connection::open(BURDBOT_DB)?;
        let stmt = "
                    INSERT OR IGNORE INTO vocaroo_enabled
                    VALUES (?);
            ";

        connection.execute(stmt, [guild_id])?;

        let enabled_str = "Enabled Vocaroo to MP3 conversions.";

        util::send_message(ctx, msg.channel_id, enabled_str, "enablevocarootomp3").await;

        return Ok(());
    }

    let already_enabled_str = "Vocaroo to MP3 conversions are already enabled on this server.";

    util::send_message(ctx, msg.channel_id, already_enabled_str, "enablevocarootomp3").await;

    Ok(())
}

#[command]
async fn isvocarootomp3enabled(ctx: &Context, msg: &Message) -> CommandResult {
    let mut data = ctx.data.write().await;
    let vocaroo_servers = match data.get_mut::<VocarooEnabled>() {
        Some(servers) => servers,
        None => return Ok(()),
    };

    let response = if vocaroo_servers.contains(&msg.guild_id.unwrap().0) {
        "Vocaroo to MP3 conversions are enabled in this server."
    } else {
        "Vocaroo to MP3 conversions are disabled in this server."
    };

    util::send_message(ctx, msg.channel_id, response, "isvocarootomp3enabled").await;

    Ok(())
}

#[command]
#[bucket("db_operations")]
async fn disablevocarootomp3(ctx: &Context, msg: &Message) -> CommandResult {
    let mut data = ctx.data.write().await;
    let vocaroo_servers = match data.get_mut::<VocarooEnabled>() {
        Some(servers) => servers,
        None => return Ok(()),
    };

    let guild_id = msg.guild_id.unwrap();
    let id = guild_id.as_u64();

    if vocaroo_servers.remove(id) {
        let connection = Connection::open(BURDBOT_DB)?;
        let stmt = "
                    DELETE FROM vocaroo_enabled
                    WHERE guild_id = ?;
            ";

        connection.execute(stmt, [id])?;

        let disabled_str = "Disabled Vocaroo to MP3 conversions.";

        util::send_message(ctx, msg.channel_id, disabled_str, "disablevocarootomp3").await;

        return Ok(());
    }

    let already_enabled_str = "Vocaroo to MP3 conversions are already disabled on this server.";

    util::send_message(ctx, msg.channel_id, already_enabled_str, "disablevocarootomp3").await;

    Ok(())
}

#[group]
#[required_permissions("manage_guild")]
#[only_in("guilds")]
#[commands(enablevocarootomp3, disablevocarootomp3, isvocarootomp3enabled)]
struct Vocaroo;
