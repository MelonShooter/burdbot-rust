use std::fmt::Debug;
use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;

use lazy_static::lazy_static;
use regex::Regex;
use serenity::client::Cache;
use serenity::client::Context;
use serenity::framework::standard::{ArgError, Args};
use serenity::http::Http;
use serenity::model::channel::Message;
use serenity::model::guild::Member;
use serenity::model::guild::Role;
use serenity::model::id::ChannelId;
use serenity::model::id::GuildId;
use serenity::model::id::RoleId;
use serenity::model::id::UserId;
use serenity::prelude::ModelError;
use serenity::Error;
use std::ops::Deref;

use log::error;

use super::error_util;
use super::error_util::error::{ArgumentConversionError, ArgumentOutOfBoundsError, ArgumentParseErrorType, NotEnoughArgumentsError};

pub mod user_search_engine;

pub struct ArgumentInfo<'a> {
    args: &'a mut Args,
    arg_pos: u32,
    args_needed: u32,
}

impl ArgumentInfo<'_> {
    pub fn new(args: &mut Args, arg_pos: u32, args_needed: u32) -> ArgumentInfo<'_> {
        ArgumentInfo { args, arg_pos, args_needed }
    }
}

pub struct BoundedArgumentInfo<'a, T: Ord + FromStr + Debug + Display + Copy> {
    args: &'a mut Args,
    arg_pos: u32,
    args_needed: u32,
    start: T,
    end: T,
}

impl<T: Ord + FromStr + Debug + Display + Copy> BoundedArgumentInfo<'_, T> {
    pub fn new(args: &mut Args, arg_pos: u32, args_needed: u32, start: T, end: T) -> BoundedArgumentInfo<'_, T> {
        BoundedArgumentInfo {
            args,
            arg_pos,
            args_needed,
            start,
            end,
        }
    }
}

pub async fn parse_bounded_arg<T>(ctx: impl AsRef<Http>, msg: &Message, arg_info: BoundedArgumentInfo<'_, T>) -> Result<T, ArgumentParseErrorType<T>>
where
    T: Ord + FromStr + Debug + Display + Copy,
{
    let start = arg_info.start;
    let end = arg_info.end;
    let args = arg_info.args;
    let arg_pos = arg_info.arg_pos;
    let args_needed = arg_info.args_needed;

    match args.parse::<T>() {
        Ok(month_number) => {
            if month_number < start || month_number > end {
                error_util::check_within_range(ctx, &msg.channel_id, month_number, arg_pos, start, end).await;

                Err(ArgumentParseErrorType::OutOfBounds(ArgumentOutOfBoundsError::new(
                    start,
                    end,
                    month_number,
                )))
            } else {
                args.advance(); // Get past the number argument.

                Ok(month_number) // Safe because of above check.
            }
        }

        Err(error) => {
            if let ArgError::Eos = error {
                // Error thrown because we've reached the end.
                error_util::not_enough_arguments(ctx, &msg.channel_id, arg_pos - 1, args_needed).await;

                Err(ArgumentParseErrorType::NotEnoughArguments(NotEnoughArgumentsError::new(
                    args_needed,
                    arg_pos - 1,
                )))
            } else {
                // Must be a parse error.
                error_util::check_within_range(ctx, &msg.channel_id, args.current().unwrap(), arg_pos, start, end).await;

                Err(ArgumentParseErrorType::ArgumentConversionError(ArgumentConversionError::new(
                    args.current().unwrap().to_owned(),
                )))
            }
        }
    }
}

fn parse_mention<T>(arg: &str, mention_matcher: &T) -> Option<u64>
where
    T: Deref<Target = Regex>,
{
    if mention_matcher.is_match(arg) {
        mention_matcher
            .captures(arg)
            .and_then(|captures| captures.get(1))
            .and_then(|mat| Some(mat.as_str().parse::<u64>().unwrap()))
    } else {
        None
    }
}

fn parse_user_mention(arg: &str) -> Option<u64> {
    lazy_static! {
        static ref USER_MENTION_MATCHER: Regex = Regex::new(r"^<@!?(\d+{17, 20})>$").unwrap();
    }

    parse_mention(arg, &USER_MENTION_MATCHER)
}

async fn id_argument_to_member(
    cache: Arc<Cache>,
    arg: &str,
    guild_id: impl Into<GuildId>,
    user_id: impl Into<UserId>,
) -> Result<Member, ArgumentParseErrorType<u32>> {
    return cache
        .clone()
        .member(guild_id, user_id)
        .await
        .ok_or(ArgumentParseErrorType::ArgumentConversionError(ArgumentConversionError::new(
            arg.to_owned(),
        )));
}

pub async fn parse_member(ctx: &Context, msg: &Message, arg_info: ArgumentInfo<'_>) -> Result<Member, ArgumentParseErrorType<u32>> {
    let args = arg_info.args;
    let cache = ctx.cache.clone();
    let guild_id = msg.guild_id.unwrap();
    let arg_pos = arg_info.arg_pos;
    let args_needed = arg_info.args_needed;

    match args.parse::<u64>() {
        Ok(user_id) => {
            if let Ok(member) = id_argument_to_member(cache.clone(), args.current().unwrap(), guild_id, user_id).await {
                args.advance();

                return Ok(member);
            }
        }
        Err(error) => {
            if let ArgError::Eos = error {
                error_util::not_enough_arguments(ctx, &msg.channel_id, arg_pos - 1, args_needed).await;

                return Err(ArgumentParseErrorType::NotEnoughArguments(NotEnoughArgumentsError::new(
                    args_needed,
                    arg_pos - 1,
                )));
            }
        }
    }

    let arg = args.current().unwrap();

    if let Some(user_id) = parse_user_mention(arg) {
        if let Ok(member) = id_argument_to_member(cache.clone(), arg, guild_id, user_id).await {
            args.advance();

            return Ok(member);
        }
    }

    if let Some(user_vec) = user_search_engine::user_id_search(ctx, guild_id.0, arg).await {
        for user_id in user_vec {
            let member_result = id_argument_to_member(cache.clone(), arg, guild_id, user_id).await;

            if let Ok(member) = member_result {
                args.advance();

                return Ok(member);
            }
        }
    }

    let msg_str = format!("Invalid argument #{}. Could not find any user with that name or ID.", arg_pos);

    send_message(ctx, &msg.channel_id, msg_str, "parse_member").await;

    Err(ArgumentParseErrorType::ArgumentConversionError(ArgumentConversionError::new(
        arg.to_owned(),
    )))
}

fn parse_role_mention(arg: &str) -> Option<u64> {
    lazy_static! {
        static ref ROLE_MENTION_MATCHER: Regex = Regex::new(r"^<@&(\d+{17, 20})>$").unwrap();
    }

    parse_mention(arg, &ROLE_MENTION_MATCHER)
}

async fn id_argument_to_role(
    cache: Arc<Cache>,
    arg: &str,
    guild_id: impl Into<GuildId>,
    role_id: impl Into<RoleId>,
) -> Result<Role, ArgumentParseErrorType<u32>> {
    return cache
        .clone()
        .role(guild_id, role_id)
        .await
        .ok_or(ArgumentParseErrorType::ArgumentConversionError(ArgumentConversionError::new(
            arg.to_owned(),
        )));
}

pub async fn parse_role(ctx: &Context, msg: &Message, arg_info: ArgumentInfo<'_>) -> Result<Role, ArgumentParseErrorType<u32>> {
    let args = arg_info.args;
    let cache = ctx.cache.clone();
    let guild_id = msg.guild_id.unwrap();
    let arg_pos = arg_info.arg_pos;
    let args_needed = arg_info.args_needed;

    match args.parse::<u64>() {
        Ok(user_id) => {
            if let Ok(role) = id_argument_to_role(cache.clone(), args.current().unwrap(), guild_id, user_id).await {
                args.advance();

                return Ok(role);
            }
        }
        Err(error) => {
            if let ArgError::Eos = error {
                error_util::not_enough_arguments(ctx, &msg.channel_id, arg_pos - 1, args_needed).await;

                return Err(ArgumentParseErrorType::NotEnoughArguments(NotEnoughArgumentsError::new(
                    args_needed,
                    arg_pos - 1,
                )));
            }
        }
    }

    let arg = args.current().unwrap();

    if let Some(user_id) = parse_role_mention(arg) {
        if let Ok(role) = id_argument_to_role(cache.clone(), arg, guild_id, user_id).await {
            args.advance();

            return Ok(role);
        }
    }

    let msg_str = format!("Invalid argument #{}. Could not find any role with that ID.", arg_pos);

    send_message(ctx, &msg.channel_id, msg_str, "parse_role").await;

    Err(ArgumentParseErrorType::ArgumentConversionError(ArgumentConversionError::new(
        arg.to_owned(),
    )))
}

pub async fn send_message(ctx: impl AsRef<Http>, ch: &ChannelId, msg: impl Display, function_name: &str) {
    if let Err(error) = ch.say(ctx, msg).await {
        if let Error::Model(ModelError::MessageTooLong(_)) = error {
            error!("{}() message too long! This shouldn't ever happen.", function_name);
        }
    }
}
