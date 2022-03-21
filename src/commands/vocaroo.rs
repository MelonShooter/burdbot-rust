use std::collections::HashSet;

use bytes::Bytes;
use lazy_static::lazy_static;
use log::{error, warn};
use regex::Regex;
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

async fn dm_and_log(ctx: &Context, string: String, error: bool) -> SerenityResult<()> {
    let str = string.as_str();

    if error {
        error!("{}", str);
    } else {
        warn!("{}", str);
    }

    let dm_channel = UserId::from(DELIBURD_ID).create_dm_channel(&ctx.http).await?;

    dm_channel.say(&ctx.http, str).await?;

    Ok(())
}

async fn handle_vocaroo_error(ctx: &Context, error: VocarooError) {
    let mut is_error = true;

    let error_message = match error {
        VocarooError::FailedHead(link) => {
            format!("Failed Vocaroo HEAD request trying to convert the link: {}. Could mean they stopped accepting it. This will break vocaroo conversions.", link)
        }
        VocarooError::FailedGet(link) => {
            format!("Failed Vocaroo GET request trying to convert the link: {}. Could mean this isn't the right URL anymore. This will break vocaroo conversions.", link)
        }
        VocarooError::FailedDownload(link, status) => {
            format!("Failed Vocaroo download trying to convert the link: {}. Response code {} was given. Check the function that resolves which CDN server vocaroo uses. Sample given in comment above handle_vocaroo_error.", link, status)
        }
        VocarooError::NoContentLength(link) => {
            format!("Vocaroo didn't send their content length in the HEAD request while trying to convert the link: {}. This will break vocaroo conversions.", link)
        }
        VocarooError::BodyToBytesFailure(link) => format!(
            "Could not convert response body to bytes while trying to convert the link: {}. This will break vocaroo conversions.",
            link
        ),
        VocarooError::ContentLengthNotNumber(link) => {
            format!("The content length returned by HEAD request was not a number while trying to convert the link: {}. This will break vocaroo converisons.", link)
        }
        VocarooError::OversizedFile(link) => {
            is_error = false;
            format!("Vocaroo file at link '{}' couldn't be converted. It is oversized ().", link)
        }
    };

    if let Err(e) = dm_and_log(ctx, error_message, is_error).await {
        error!("Error encountered DMing vocaroo error to DELIBURD. Error: {}", e);
    }
}

async fn process_vocaroo_id(client: &Client, vocaroo_id: &str) -> Result<Bytes, VocarooError> {
    let url = format!("https://media1.vocaroo.com/mp3/{}", vocaroo_id);
    let url_str = url.as_str();

    let head_response = match client.head(url_str).send().await.ok() {
        Some(response) => response,
        None => return Err(VocarooError::FailedHead(url)),
    };

    let content_length_str = match head_response.headers().get("Content-Length").map(|val| val.to_str()) {
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
    let data_lock = ctx.data.clone();
    let data = data_lock.read().await;
    let vocaroo_servers = data.get::<VocarooEnabled>();

    if let Some(servers) = vocaroo_servers {
        let guild_id = match msg.guild_id {
            Some(id) => id,
            None => return,
        };
        let id = guild_id.as_u64();

        if servers.contains(id) {
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

            let http = ctx.http.clone();
            let msg_ref = MessageReference::from(msg);
            let channel_id = msg.channel_id;
            let user_id = msg.author.id.0;

            match process_vocaroo_id(&VOCAROO_CLIENT, vocaroo_id.as_str()).await {
                Ok(vocaroo_data) => {
                    let _ = channel_id
                        .send_message(&http, |msg_builder| {
                            msg_builder.add_file((&vocaroo_data[..], "vocaroo-to-mp3.mp3"));

                            let mut msg_str = String::with_capacity(96);
                            msg_str.push_str("Here is <@");
                            msg_str.push_str(user_id.to_string().as_str());
                            msg_str.push_str(">'s vocaroo link as an MP3 file. This is limited to 1 per message.");

                            msg_builder.content(msg_str.as_str());

                            msg_builder.reference_message(msg_ref)
                        })
                        .await;
                }
                Err(error) => handle_vocaroo_error(ctx, error).await,
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
        let data_lock = ctx.data.clone();
        let mut data = data_lock.write().await;

        data.insert::<VocarooEnabled>(servers);
    }
}

#[command]
#[bucket("db_operations")]
async fn enablevocarootomp3(ctx: &Context, msg: &Message) -> CommandResult {
    let data_lock = ctx.data.clone();
    let mut data = data_lock.write().await;
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

        util::send_message(ctx, &msg.channel_id, enabled_str, "enablevocarootomp3").await;

        return Ok(());
    }

    let already_enabled_str = "Vocaroo to MP3 conversions are already enabled on this server.";

    util::send_message(ctx, &msg.channel_id, already_enabled_str, "enablevocarootomp3").await;

    Ok(())
}

#[command]
async fn isvocarootomp3enabled(ctx: &Context, msg: &Message) -> CommandResult {
    let data_lock = ctx.data.clone();
    let mut data = data_lock.write().await;
    let vocaroo_servers = match data.get_mut::<VocarooEnabled>() {
        Some(servers) => servers,
        None => return Ok(()),
    };

    let response = if vocaroo_servers.contains(&msg.guild_id.unwrap().0) {
        "Vocaroo to MP3 conversions are enabled in this server."
    } else {
        "Vocaroo to MP3 conversions are disabled in this server."
    };

    util::send_message(ctx, &msg.channel_id, response, "isvocarootomp3enabled").await;

    Ok(())
}

#[command]
#[bucket("db_operations")]
async fn disablevocarootomp3(ctx: &Context, msg: &Message) -> CommandResult {
    let data_lock = ctx.data.clone();
    let mut data = data_lock.write().await;
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

        util::send_message(ctx, &msg.channel_id, disabled_str, "disablevocarootomp3").await;

        return Ok(());
    }

    let already_enabled_str = "Vocaroo to MP3 conversions are already disabled on this server.";

    util::send_message(ctx, &msg.channel_id, already_enabled_str, "disablevocarootomp3").await;

    Ok(())
}

#[group]
#[required_permissions("manage_guild")]
#[only_in("guilds")]
#[commands(enablevocarootomp3, disablevocarootomp3, isvocarootomp3enabled)]
struct Vocaroo;
