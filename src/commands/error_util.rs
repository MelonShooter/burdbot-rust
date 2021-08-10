use std::fmt::Display;

use serenity::client::Context;
use serenity::http::Http;
use serenity::model::id::ChannelId;

use crate::commands::util;
use crate::DELIBURD_ID;

pub mod error;

pub async fn not_enough_arguments(ctx: impl AsRef<Http>, ch: &ChannelId, arg_count: u32, args_needed: u32) {
    let args_needed_message;
    let arg_count_message;

    if args_needed != 1 {
        args_needed_message = "s are";
    } else {
        args_needed_message = " is";
    }

    if arg_count != 1 {
        arg_count_message = "s were";
    } else {
        arg_count_message = " was";
    }

    let not_enough_arguments_message = format!(
        "Invalid number of arguments provided. \
            {} argument{} needed. {} argument{} provided.",
        args_needed, args_needed_message, arg_count, arg_count_message
    );

    util::send_message(ctx, ch, not_enough_arguments_message, "not_enough_arguments").await;
}

pub async fn check_within_range<T: Display, U: Display>(ctx: impl AsRef<Http>, ch: &ChannelId, arg: T, arg_pos: u32, start: U, end: U) {
    let invalid_range_message = format!(
        "Invalid argument #{} provided. \
            The range should be within {} and {} (inclusive). \
            The argument given was {}.",
        arg_pos, start, end, arg
    );

    util::send_message(ctx, ch, invalid_range_message, "number_within_range").await;
}

async fn deliburd_in_server(ctx: &Context, ch: &ChannelId) -> bool {
    if let Ok(channel) = ch.to_channel(ctx).await {
        if let Some(guild_channel) = channel.guild() {
            if ctx.cache.member(guild_channel.guild_id, DELIBURD_ID).await.is_some() {
                return true;
            }
        }
    }

    false
}

pub async fn generic_fail(ctx: &Context, ch: &ChannelId) {
    let fail_message;

    if deliburd_in_server(ctx, ch).await {
        fail_message = format!("Something went wrong. <@{}> has been notified about this.", DELIBURD_ID);
    } else {
        if let Some(owner) = ctx.cache.user(DELIBURD_ID).await {
            fail_message = format!("Something went wrong. Please contact the owner of the bot, {}.", owner.tag());
        } else {
            fail_message = format!(
                "Something went wrong. Please contact the owner of the bot. Their user ID is {}.",
                DELIBURD_ID
            );
        }
    }

    util::send_message(ctx.http.clone(), ch, fail_message, "generic_fail").await;
}

/*pub async fn unknown_command_message(ctx: impl AsRef<Http>, ch: &ChannelId) {
    let unknown_command_message = "Unknown command. Type the help command to get the list of commands.";

    util::send_message(ctx, ch, unknown_command_message, "unknown_command_message").await;
}*/
