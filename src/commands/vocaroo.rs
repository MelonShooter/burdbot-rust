use std::collections::HashSet;
use std::fmt::Display;

use bytes::Bytes;
use lazy_static::lazy_static;
use log::{debug, error, warn};
use regex::Regex;
use reqwest::header::HeaderValue;
use reqwest::Client;
use rusqlite::{Connection, Error as SqliteError};
use serenity::client::Context;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::CommandResult;
use serenity::model::channel::{Message, MessageReference};
use serenity::model::id::UserId;
use serenity::prelude::TypeMapKey;
use serenity::Result as SerenityResult;

use crate::{BURDBOT_DB, DELIBURD_ID};

use super::util;

const MAX_VOCAROO_RECORDING_SIZE: u32 = (1 << 20) * 5; // 5MB

struct VocarooEnabled;

impl TypeMapKey for VocarooEnabled {
    type Value = HashSet<u64>;
}

#[non_exhaustive]
#[derive(Debug)]
enum VocarooError {
    FailedHead(String),
    FailedGet(String),
    FailedDownload(String, u16),
    NoContentLength(String),
    ContentLengthNotNumber(String),
    BodyToBytesFailure(String),
    OversizedFile(String),
}

async fn dm_and_log<T: AsRef<str> + Display>(ctx: &Context, string: T, error: bool) -> SerenityResult<()> {
    if error {
        error!("{string}");
    } else {
        warn!("{string}");
    }

    let dm_channel = UserId::from(DELIBURD_ID).create_dm_channel(&ctx.http).await?;

    dm_channel.say(&ctx.http, string).await?;

    Ok(())
}

async fn dm_and_log_handled<T: AsRef<str> + Display>(ctx: &Context, error_message: T, is_error: bool) {
    if let Err(e) = dm_and_log(ctx, error_message, is_error).await {
        error!("Error encountered DMing vocaroo error to DELIBURD. Error: {e}");
    }
}

async fn handle_vocaroo_error(ctx: &Context, msg: &Message, error: VocarooError) {
    let mut is_error = true;

    let error_message = match error {
        VocarooError::FailedHead(link) => {
            format!("Failed Vocaroo HEAD request trying to convert the link: {link}. Could mean they stopped accepting it. This will break vocaroo conversions.")
        }
        VocarooError::FailedGet(link) => {
            format!("Failed Vocaroo GET request trying to convert the link: {link}. Could mean this isn't the right URL anymore. This will break vocaroo conversions.")
        }
        VocarooError::FailedDownload(link, status) => {
            is_error = false;
            format!("Warning: Failed Vocaroo download trying to convert the link: {link}. Response code {status} was given.")
        }
        VocarooError::NoContentLength(link) => {
            format!("Vocaroo didn't send their content length in the HEAD request while trying to convert the link: {link}. This will break vocaroo conversions.")
        }
        VocarooError::BodyToBytesFailure(link) => {
            format!("Could not convert response body to bytes while trying to convert the link: {link}. This will break vocaroo conversions.")
        }
        VocarooError::ContentLengthNotNumber(link) => {
            format!("The content length returned by HEAD request was not a number while trying to convert the link: {link}. This will break vocaroo conversions.")
        }
        VocarooError::OversizedFile(link) => {
            is_error = false;
            format!("Warning: Vocaroo file at link '{link}' couldn't be converted. It is oversized. This can generally be ignored.")
        }
    };

    if let Err(err) = msg.react(&ctx.http, '❌').await {
        let link = msg.link();

        debug!("Failed to react to vocaroo recording that errored. Error: {err}. Message link: {link}.");
    }

    dm_and_log_handled(ctx, error_message, is_error).await;
}

async fn process_vocaroo_id(ctx: &Context, client: &Client, vocaroo_id: &str) -> Result<Bytes, VocarooError> {
    let url = format!("https://media.vocaroo.com/mp3/{vocaroo_id}");

    match download_vocaroo(client, url).await {
        Ok(bytes) => Ok(bytes),
        Err(err) => match err {
            VocarooError::FailedDownload(link, status) => {
                let msg = format!("Warning: Failed to download vocaroo recording from https://media.vocaroo.com for link: {link}. Status {status} given. Falling back to https://media1.vocaroo.com... If status 404 was given, this probably just means the vocaroo recording expired unless the fallback link is successful. If the fallback link is successful, then this means you should check the rules again to see if they're accurate now. The JS code currently doesn't actually match which CDN server to use but it appears all new recordings are at https://media.vocaroo.com, not https://media1.vocaroo.com. Check the current code by looking up usages of 'mediaMP3FileURL' on the site's JS code in the script at the bottom of the body.");

                dm_and_log_handled(ctx, msg, false).await;

                download_vocaroo(client, format!("https://media1.vocaroo.com/mp3/{vocaroo_id}")).await
            }
            _ => Err(err),
        },
    }
}

async fn download_vocaroo(client: &Client, url: String) -> Result<Bytes, VocarooError> {
    let url_str = url.as_str();
    let head_response = match client.head(url_str).send().await.ok() {
        Some(response) => response,
        None => return Err(VocarooError::FailedHead(url)),
    };
    let content_length_str = match head_response.headers().get("Content-Length").map(HeaderValue::to_str) {
        Some(parsed_result) => match parsed_result.ok() {
            Some(str) => str,
            None => return Err(VocarooError::ContentLengthNotNumber(url)),
        },
        None => return Err(VocarooError::NoContentLength(url)),
    };
    let content_length = match content_length_str.parse::<u32>().ok() {
        Some(len) => len,
        None => return Err(VocarooError::ContentLengthNotNumber(url)),
    };
    if content_length <= MAX_VOCAROO_RECORDING_SIZE {
        let get_response = match client.get(url_str).send().await.ok() {
            Some(response) => {
                if response.status().is_success() {
                    response
                } else {
                    return Err(VocarooError::FailedDownload(url, response.status().as_u16()));
                }
            }
            None => return Err(VocarooError::FailedGet(url)),
        };

        let bytes = match get_response.bytes().await.ok() {
            Some(b) => b,
            None => return Err(VocarooError::BodyToBytesFailure(url)),
        };

        Ok(bytes)
    } else {
        Err(VocarooError::OversizedFile(url))
    }
}

pub async fn on_message_received(ctx: &Context, msg: &Message) {
    let data = ctx.data.read().await;
    let vocaroo_servers = data.get::<VocarooEnabled>();

    if let Some(servers) = vocaroo_servers {
        let id = match msg.guild_id {
            Some(id) => *id.as_u64(),
            None => return,
        };

        if servers.contains(&id) {
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

            match process_vocaroo_id(ctx, &VOCAROO_CLIENT, vocaroo_id.as_str()).await {
                Ok(vocaroo_data) => {
                    let msg_result = channel_id
                        .send_message(&ctx.http, |msg_builder| {
                            msg_builder.add_file((&vocaroo_data[..], "vocaroo-to-mp3.mp3"));

                            let mut msg_str = String::with_capacity(96);
                            msg_str.push_str("Here is <@");
                            msg_str.push_str(user_id.to_string().as_str());
                            msg_str.push_str(">'s vocaroo link as an MP3 file. This is limited to 1 per message.");

                            msg_builder.content(msg_str.as_str());

                            msg_builder.reference_message(msg_ref)
                        })
                        .await;

                    if let Err(e) = msg_result {
                        warn!("Couldn't send vocaroo message in channel {channel_id} because of error: {e}");
                    }
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
