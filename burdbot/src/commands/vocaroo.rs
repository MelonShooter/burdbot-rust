use std::collections::HashSet;

use bytes::Bytes;
use log::{debug, error, warn};
use rusqlite::Connection;
use serenity::builder::CreateMessage;
use serenity::client::Context;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::CommandResult;
use serenity::model::channel::{Message, MessageReference};
use serenity::model::id::{ChannelId, GuildId};
use serenity::prelude::TypeMapKey;
use serenity::Error;

use crate::commands::error_util;
use crate::vocaroo::VocarooError;
use crate::BURDBOT_DB;
use crate::{util, vocaroo};

const MAX_VOCAROO_RECORDING_SIZE: u64 = (1 << 20) * 5; // 5MB

struct VocarooEnabled;

impl TypeMapKey for VocarooEnabled {
    type Value = HashSet<u64>;
}

async fn handle_vocaroo_error(ctx: &Context, msg: &Message, error: VocarooError<'_>) {
    match error {
        VocarooError::MalformedUrl(_) => return, // We don't care about this error.
        VocarooError::FailedDownload(_, _) => warn!("{error}"),
        VocarooError::OversizedFile(_, _) | VocarooError::ContentTypeNotMp3(_) => debug!("{error}"),
        _ => {
            error!("{error}");
            error_util::generic_fail(ctx, msg.channel_id).await;
        },
    };

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
    let msg_result = channel_id.send_message(&ctx.http, |c| format_recording(c, &vocaroo_data[..], user_id, msg_ref)).await;

    match msg_result {
        Ok(_) => (),
        Err(Error::Http(err)) => {
            debug!(
                "Couldn't send vocaroo message in channel {channel_id} in server {guild_id} because of HTTP error: {err:?}). \
                 This is generally due to a lack of permissions."
            )
        },
        Err(err) => warn!("Couldn't send vocaroo message in channel {channel_id} in server {guild_id} because of error: {err}"),
    };
}

pub async fn on_message_received(ctx: &Context, msg: &Message) {
    // Some early exits.
    let content = msg.content.as_str();

    if content.len() > 40 || !content.starts_with("http") {
        return;
    }

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
        let channel_id = msg.channel_id;
        let user_id = msg.author.id.0;

        match vocaroo::download_vocaroo(content, MAX_VOCAROO_RECORDING_SIZE).await {
            Ok(vocaroo_data) => {
                send_recording(ctx, channel_id, guild_id, vocaroo_data, user_id, msg_ref).await;
            },
            Err(error) => handle_vocaroo_error(ctx, msg, error).await,
        }
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
