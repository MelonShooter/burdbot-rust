use std::fmt::Display;

use serenity::http::Http;
use serenity::model::id::ChannelId;

use crate::util;

pub async fn not_enough_arguments(
    ctx: impl AsRef<Http>, ch: ChannelId, arg_count: usize, args_needed: usize,
) {
    let args_needed_message = if args_needed == 1 { " is" } else { "s are" };
    let arg_count_message = if arg_count == 1 { " was" } else { "s were" };

    let not_enough_arguments_message = format!(
        "Invalid number of arguments provided. \
            {args_needed} argument{args_needed_message} needed. {arg_count} argument{arg_count_message} provided."
    );

    util::send_message(ctx, ch, not_enough_arguments_message, "not_enough_arguments").await;
}

pub async fn check_within_range<T: Display, U: Display>(
    ctx: impl AsRef<Http>, ch: ChannelId, arg: T, arg_pos: usize, start: U, end: U,
) {
    let invalid_range_message = format!(
        "Invalid argument #{arg_pos} provided. \
            The range should be within {start} and {end} (inclusive). \
            The argument given was {arg}.",
    );

    util::send_message(ctx, ch, invalid_range_message, "number_within_range").await;
}
