use std::fmt::Display;

use serenity::client::Context;
use serenity::http::Http;
use serenity::model::id::ChannelId;

use crate::commands::util;

pub async fn not_enough_arguments(ctx: impl AsRef<Http>, ch: ChannelId, arg_count: usize, args_needed: usize) {
    let args_needed_message = if args_needed == 1 { " is" } else { "s are" };
    let arg_count_message = if arg_count == 1 { " was" } else { "s were" };

    let not_enough_arguments_message = format!(
        "Invalid number of arguments provided. \
            {} argument{} needed. {} argument{} provided.",
        args_needed, args_needed_message, arg_count, arg_count_message
    );

    util::send_message(ctx, ch, not_enough_arguments_message, "not_enough_arguments").await;
}

pub async fn check_within_range<T: Display, U: Display>(ctx: impl AsRef<Http>, ch: ChannelId, arg: T, arg_pos: usize, start: U, end: U) {
    let invalid_range_message = format!(
        "Invalid argument #{} provided. \
            The range should be within {} and {} (inclusive). \
            The argument given was {}.",
        arg_pos, start, end, arg
    );

    util::send_message(ctx, ch, invalid_range_message, "number_within_range").await;
}

pub async fn generic_fail(ctx: &Context, ch: ChannelId) {
    util::send_message(
        &ctx.http,
        ch,
        "Something went wrong. The owner of the bot has been notified of this.",
        "generic_fail",
    )
    .await;
}

/*pub async fn unknown_command_message(ctx: impl AsRef<Http>, ch: ChannelId) {
    let unknown_command_message = "Unknown command. Type the help command to get the list of commands.";

    util::send_message(ctx, ch, unknown_command_message, "unknown_command_message").await;
}*/
