use lazy_static::lazy_static;
use log::info;
use regex::Regex;
use reqwest::IntoUrl;
use std::fmt::Display;
use std::io;
use tokio::process::Command;

use serenity::Error;
use serenity::all::MessageId;
use serenity::client::Cache;
use serenity::http::Http;
use serenity::model::Permissions;
use serenity::model::channel::Message;
use serenity::model::id::ChannelId;
use serenity::model::id::GuildId;
use serenity::model::id::UserId;
use serenity::prelude::ModelError;

use log::error;

// Gets the IDs from a message link
pub fn get_ids_from_msg_link(link: impl AsRef<str>) -> Option<(GuildId, ChannelId, MessageId)> {
    lazy_static! {
        static ref ID_MATCHER: Regex =
            Regex::new(r"https://discord\.com/channels/(\d{8,})/(\d{8,})/(\d{8,})").unwrap();
    }

    let c = ID_MATCHER.captures(link.as_ref())?;
    let guild_id = c.get(1)?.as_str().parse::<GuildId>().ok()?;
    let channel_id = c.get(2)?.as_str().parse::<ChannelId>().ok()?;
    let message_id = c.get(3)?.as_str().parse::<MessageId>().ok()?;

    Some((guild_id, channel_id, message_id))
}

pub fn check_message_sending(res: serenity::Result<Message>, function_name: &str) {
    if let Err(Error::Model(ModelError::MessageTooLong(_))) = res {
        error!("{}() message too long! This shouldn't ever happen.", function_name);
    }
}

pub async fn send_message(
    ctx: impl AsRef<Http>, ch: ChannelId, msg: impl Display, function_name: &str,
) {
    let ctx = ctx.as_ref();

    check_message_sending(ch.say(ctx, msg.to_string()).await, function_name);
}

pub async fn get_member_permissions<T: AsRef<Cache>>(
    cache: T, guild_id: GuildId, user_id: impl Into<UserId>,
) -> Option<Permissions> {
    let guild = cache.as_ref().guild(guild_id)?;

    guild.members.get(&user_id.into()).map(|member| {
        member
            .roles
            .iter()
            .flat_map(|id| guild.roles.get(id).map(|role| role.permissions)) // Map role ID to Permissions
            .fold(Permissions::empty(), |acc, permissions| acc | permissions)
    })
}

/// Fetches HTML from a site bypassing anti-scrapers
pub async fn anti_scraper_get_html(url: impl IntoUrl) -> std::io::Result<String> {
    let out = Command::new("lynx").arg("-source").arg(url.as_str()).output().await?.stdout;
    String::from_utf8(out).map_err(io::Error::other)
}

/// Fetches file from site
pub async fn anti_scraper_download_file(url: impl IntoUrl) -> std::io::Result<Vec<u8>> {
    info!("Downloading file from {}", url.as_str());

    Ok(Command::new("wget").arg("-qO").arg("-").arg(url.as_str()).output().await?.stdout)
}
