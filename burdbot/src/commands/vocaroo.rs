use std::borrow::Cow;
use std::collections::HashSet;

use bytes::Bytes;
use log::{debug, error, warn};
use rusqlite::Connection;
use serenity::builder::CreateMessage;
use serenity::client::Context;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::CommandResult;
use serenity::model::channel::{AttachmentType, Message, MessageReference, ReactionType};
use serenity::prelude::TypeMapKey;
use serenity::{json, Error};

use crate::commands::error_util;
use crate::vocaroo::VocarooError;
use crate::BURDBOT_DB;
use crate::{util, vocaroo};

const TOTAL_MAX_VOCAROO_SIZE: usize = (1 << 20) * 6; // 6MiB
const VOCAROO_ATTACHMENT_LIMIT: usize = 8;

struct VocarooEnabled;

impl TypeMapKey for VocarooEnabled {
    type Value = HashSet<u64>;
}

fn log_and_check_is_severe(error: &VocarooError) -> bool {
    match error {
        VocarooError::InvalidUrls(_) => debug!("Encountered link that wasn't recognized as a vocaroo link: {error}"),
        VocarooError::FailedDownload(_, _) => warn!("{error}"),
        VocarooError::OverSizeLimit(_, _) | VocarooError::ContentTypeNotMp3(_) => debug!("{error}"),
        _ => {
            error!("{error}");

            return true;
        },
    };

    false
}

pub async fn on_message_received(ctx: &Context, msg: &Message) {
    // Some early exits.
    let content = msg.content.as_str();
    let first_link_idx = match content.find("http") {
        Some(idx) => idx,
        None => return,
    };

    let data = ctx.data.read().await;
    let vocaroo_servers = data.get::<VocarooEnabled>();

    if let Some(servers) = vocaroo_servers {
        let guild_id = match msg.guild_id {
            Some(id) => id,
            None => return,
        };

        if !servers.contains(&guild_id.0) {
            return;
        }

        let msg_ref = MessageReference::from(msg);
        let user_id = msg.author.id.0;

        // This needs to be in its own function due to a bug in the compiler
        // causing a very weird error when a closure is directly used.
        fn filter_severe(recording: &vocaroo::Result<Bytes>) -> bool {
            !matches!(recording, Err(ref err) if log_and_check_is_severe(err) )
        }

        let mut recording_count = 0;
        let mut err_count = 0;
        let mut recordings = vocaroo::download_vocaroos(&content[first_link_idx..], TOTAL_MAX_VOCAROO_SIZE, VOCAROO_ATTACHMENT_LIMIT)
            .await
            .filter(filter_severe)
            .peekable();

        let mut message = match recordings.peek() {
            Some(_) => CreateMessage::default(),
            None => return,
        };

        for recording in recordings {
            recording_count += 1;

            match recording {
                Ok(recording) => {
                    let recording = Cow::from(recording.to_vec());
                    let attachment = AttachmentType::Bytes { data: recording, filename: "vocaroo.mp3".to_string() };

                    message.add_file(attachment);
                },
                Err(_) => err_count += 1,
            };
        }

        async fn error_react<T: Into<ReactionType>>(ctx: &Context, msg: &Message, reaction: T) {
            error_util::generic_fail(ctx, msg.channel_id).await;

            if let Err(err) = msg.react(&ctx.http, reaction).await {
                let link = msg.link();

                debug!("Failed to react to a vocaroo recording that errored. Error: {err}. Message link: {link}.");
            }
        }

        if err_count == recording_count {
            error_react(ctx, msg, '❌').await;

            return;
        } else if err_count > 0 {
            error_react(ctx, msg, ReactionType::Unicode("⚠️".to_string())).await;
        }

        let id = msg.channel_id.0;
        let msg_str = format!(
            "Here is <@{user_id}>'s vocaroo link as an MP3 \
             file. This is limited to 1 per message."
        );

        message.content(msg_str);
        message.reference_message(msg_ref);

        let map = json::hashmap_to_json_map(message.0);

        // we need a length check here

        match ctx.http.send_files(id, message.2, &map).await {
            Ok(_) => (),
            Err(Error::Http(err)) => {
                debug!(
                    "Couldn't send vocaroo message in channel {id} in server {guild_id} because of HTTP error: {err:?}). \
                     This is generally due to a lack of permissions or the message was too large."
                )
            },
            Err(err) => warn!("Couldn't send vocaroo message in channel {id} in server {guild_id} because of error: {err}"),
        };
    }
}

fn get_all_vocaroo_servers() -> rusqlite::Result<HashSet<u64>> {
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
