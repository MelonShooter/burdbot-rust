use std::fmt::Display;

use serenity::client::Cache;
use serenity::http::Http;
use serenity::model::channel::Message;
use serenity::model::id::ChannelId;
use serenity::model::id::GuildId;
use serenity::model::id::UserId;
use serenity::model::Permissions;
use serenity::prelude::ModelError;
use serenity::Error;

use log::error;

pub fn check_message_sending(res: serenity::Result<Message>, function_name: &str) {
    if let Err(Error::Model(ModelError::MessageTooLong(_))) = res {
        error!("{}() message too long! This shouldn't ever happen.", function_name);
    }
}

pub async fn send_message(ctx: impl AsRef<Http>, ch: ChannelId, msg: impl Display, function_name: &str) {
    let ctx = ctx.as_ref();

    check_message_sending(ch.say(ctx, msg).await, function_name);
}

pub async fn get_member_permissions<T: AsRef<Cache>>(cache: T, guild_id: GuildId, user_id: impl Into<UserId>) -> Option<Permissions> {
    cache
        .as_ref()
        .guild_field(guild_id, |guild| {
            guild.members.get(&user_id.into()).map(|member| {
                member
                    .roles
                    .iter()
                    .flat_map(|id| guild.roles.get(id).map(|role| role.permissions)) // Map role ID to Permissions
                    .fold(Permissions::empty(), |acc, permissions| acc | permissions)
            })
        })
        .flatten()
}
