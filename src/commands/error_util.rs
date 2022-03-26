use std::error::Error;
use std::fmt::Display;

use log::{error, warn};
use serenity::client::Context;
use serenity::http::Http;
use serenity::model::id::{ChannelId, UserId};
use serenity::Error as SerenityError;

use crate::commands::util;
use crate::DELIBURD_ID;

pub mod error;

async fn dm_and_log<T: AsRef<str> + Display>(ctx: &Context, string: T, issue_type: IssueType) -> Result<(), SerenityError> {
    match issue_type {
        IssueType::Warning => warn!("{string}"),
        IssueType::Error => error!("{string}"),
    }

    let dm_channel = UserId::from(DELIBURD_ID).create_dm_channel(&ctx.http).await?;

    dm_channel.say(&ctx.http, string).await?;

    Ok(())
}

async fn dm_and_log_handled<T: AsRef<str> + Display>(ctx: &Context, error_message: T, issue_type: IssueType) {
    if let Err(e) = dm_and_log(ctx, error_message, issue_type).await {
        error!("Error encountered DMing error to DELIBURD. Error: {e}");
    }
}

#[derive(Debug)]
pub enum IssueType {
    Warning,
    Error,
}

pub async fn dm_issue<S, T: Error>(
    ctx: &Context,
    identifier: &str,
    result: Result<S, T>,
    additional_info: &str,
    issue_type: IssueType,
) -> Result<S, T> {
    dm_issue_no_return(ctx, identifier, &result, additional_info, issue_type).await;

    result
}

pub async fn dm_issue_no_return<S, T: Error>(ctx: &Context, identifier: &str, result: &Result<S, T>, additional_info: &str, issue_type: IssueType) {
    if let Err(err) = result {
        let message = format!("Error encountered in the command/module '{identifier}'. Error: {err}\nAdditional information: {additional_info}");

        dm_and_log_handled(ctx, message, issue_type).await;
    }
}

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

async fn deliburd_in_server(ctx: &Context, ch: ChannelId) -> bool {
    if let Ok(channel) = ch.to_channel(ctx).await {
        if let Some(guild_channel) = channel.guild() {
            if ctx.cache.member(guild_channel.guild_id, DELIBURD_ID).await.is_some() {
                return true;
            }
        }
    }

    false
}

pub async fn generic_fail(ctx: &Context, ch: ChannelId) {
    let fail_message;

    if deliburd_in_server(ctx, ch).await {
        fail_message = format!("Something went wrong. <@{}> has been notified about this.", DELIBURD_ID);
    } else if let Some(owner) = ctx.cache.user(DELIBURD_ID).await {
        fail_message = format!("Something went wrong. Please contact the owner of the bot, {}.", owner.tag());
    } else {
        fail_message = format!(
            "Something went wrong. Please contact the owner of the bot. Their user ID is {}.",
            DELIBURD_ID
        );
    }

    util::send_message(&ctx.http, ch, fail_message, "generic_fail").await;
}

/*pub async fn unknown_command_message(ctx: impl AsRef<Http>, ch: ChannelId) {
    let unknown_command_message = "Unknown command. Type the help command to get the list of commands.";

    util::send_message(ctx, ch, unknown_command_message, "unknown_command_message").await;
}*/
