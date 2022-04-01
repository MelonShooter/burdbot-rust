use std::fmt::Display;
use std::hash::Hash;
use std::str::FromStr;

use lazy_static::lazy_static;
use regex::Regex;
use serenity::client::Cache;
use serenity::client::Context;
use serenity::framework::standard::{ArgError, Args};
use serenity::http::Http;
use serenity::model::channel::Message;
use serenity::model::guild::Member;
use serenity::model::id::ChannelId;
use serenity::model::id::GuildId;
use serenity::model::id::RoleId;
use serenity::model::id::UserId;
use serenity::model::Permissions;
use serenity::prelude::ModelError;
use serenity::utils::Colour;
use serenity::Error;
use serenity::Result as SerenityResult;
use std::ops::Deref;
use strum_macros::Display;
use strum_macros::EnumProperty;

use log::error;

use super::error_util;
use crate::error::BadOptionError;
use crate::error::{ArgumentConversionError, ArgumentOutOfBoundsError, ArgumentParseError, NotEnoughArgumentsError};

pub mod user_search_engine;

// TODO: UPDATE THIS TO TAKE ADVANTAGE OF FROM (use ?) AND NEW DISPLAY IMPLS EVERYWHERE
// TODO: Add different conversion types
#[derive(Display, Debug, EnumProperty, Copy, Clone)]
pub enum ConversionType {
    // add conversions and info properties
    Number,
    Member,
    Role,
    NonSelfMember,
}
pub struct ArgumentInfo<'a> {
    args: &'a mut Args,
    arg_pos: usize,
    args_needed: usize,
}

impl ArgumentInfo<'_> {
    pub fn new(args: &mut Args, arg_pos: usize, args_needed: usize) -> ArgumentInfo<'_> {
        ArgumentInfo { args, arg_pos, args_needed }
    }
}

pub struct BoundedArgumentInfo<'a> {
    args: &'a mut Args,
    arg_pos: usize,
    args_needed: usize,
    start: i64,
    end: i64,
}

impl BoundedArgumentInfo<'_> {
    pub fn new(args: &mut Args, arg_pos: usize, args_needed: usize, start: i64, end: i64) -> BoundedArgumentInfo<'_> {
        BoundedArgumentInfo {
            args,
            arg_pos,
            args_needed,
            start,
            end,
        }
    }
}

pub async fn parse_bounded_arg(ctx: impl AsRef<Http>, msg: &Message, arg_info: BoundedArgumentInfo<'_>) -> Result<i64, ArgumentParseError> {
    let BoundedArgumentInfo {
        start,
        end,
        args,
        arg_pos,
        args_needed,
    } = arg_info;

    match args.parse::<i64>() {
        Ok(month_number) => {
            if month_number < start || month_number > end {
                error_util::check_within_range(ctx, msg.channel_id, month_number, arg_pos, start, end).await;

                Err(ArgumentParseError::OutOfBounds(ArgumentOutOfBoundsError::new(
                    start,
                    end,
                    month_number,
                    arg_pos,
                )))
            } else {
                args.advance(); // Get past the number argument.

                Ok(month_number) // Safe because of above check.
            }
        }

        Err(error) => {
            if let ArgError::Eos = error {
                // Error thrown because we've reached the end.
                error_util::not_enough_arguments(ctx, msg.channel_id, arg_pos - 1, args_needed).await;

                Err(ArgumentParseError::NotEnoughArguments(NotEnoughArgumentsError::new(
                    args_needed,
                    arg_pos - 1,
                )))
            } else {
                // Must be a parse error.
                error_util::check_within_range(ctx, msg.channel_id, args.current().unwrap(), arg_pos, start, end).await;

                Err(ArgumentParseError::ArgumentConversionError(ArgumentConversionError::new(
                    arg_pos,
                    args.current().unwrap().to_owned(),
                    ConversionType::Number,
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
            .map(|mat| mat.as_str().parse::<u64>().unwrap())
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

async fn id_argument_to_member<T: AsRef<Cache>>(
    cache: T,
    arg_pos: usize,
    arg: &str,
    guild_id: impl Into<GuildId>,
    user_id: impl Into<UserId>,
) -> Result<Member, ArgumentParseError> {
    return cache
        .as_ref()
        .member(guild_id, user_id)
        .await
        .ok_or_else(|| ArgumentConversionError::new(arg_pos, arg.to_owned(), ConversionType::Member).into());
}

pub async fn parse_member(ctx: &Context, msg: &Message, arg_info: ArgumentInfo<'_>) -> Result<Member, ArgumentParseError> {
    let cache = &ctx.cache;
    let guild_id = msg.guild_id.unwrap();
    let ArgumentInfo { args, arg_pos, args_needed } = arg_info;

    match args.parse::<u64>() {
        Ok(user_id) => {
            if let Ok(member) = id_argument_to_member(cache, arg_pos, args.current().unwrap(), guild_id, user_id).await {
                args.advance();

                return Ok(member);
            }
        }
        Err(error) => {
            if let ArgError::Eos = error {
                error_util::not_enough_arguments(ctx, msg.channel_id, arg_pos - 1, args_needed).await;

                return Err(ArgumentParseError::NotEnoughArguments(NotEnoughArgumentsError::new(
                    args_needed,
                    arg_pos - 1,
                )));
            }
        }
    }

    let arg = args.current().unwrap();

    if let Some(user_id) = parse_user_mention(arg) {
        if let Ok(member) = id_argument_to_member(cache, arg_pos, arg, guild_id, user_id).await {
            args.advance();

            return Ok(member);
        }
    }

    if let Some(user_vec) = user_search_engine::user_id_search(ctx, guild_id.0, arg).await {
        for user_id in user_vec {
            let member_result = id_argument_to_member(cache, arg_pos, arg, guild_id, user_id).await;

            if let Ok(member) = member_result {
                args.advance();

                return Ok(member);
            }
        }
    }

    let msg_str = format!("Invalid argument #{}. Could not find any user with that name or ID.", arg_pos);

    send_message(ctx, msg.channel_id, msg_str, "parse_member").await;

    Err(ArgumentParseError::ArgumentConversionError(ArgumentConversionError::new(
        arg_pos,
        arg.to_owned(),
        ConversionType::Member,
    )))
}

fn parse_role_mention(arg: &str) -> Option<u64> {
    lazy_static! {
        static ref ROLE_MENTION_MATCHER: Regex = Regex::new(r"^<@&(\d+{17, 20})>$").unwrap();
    }

    parse_mention(arg, &ROLE_MENTION_MATCHER)
}

async fn bad_option_message<'a, T: Iterator>(ctx: &Context, msg: &Message, arg_pos: usize, choices: T) -> String
where
    T::Item: Display,
{
    let choices = choices.map(|choice| choice.to_string() + " ").collect::<String>();
    let bad_option_title = format!("Invalid argument #{}. Not one of the possible options.", arg_pos);

    let res = msg
        .channel_id
        .send_message(&ctx.http, |m| {
            m.embed(|embed| {
                embed.title(bad_option_title);
                embed.color(Colour::RED);

                embed.field("Possible options are", choices.as_str(), true)
            })
        })
        .await;

    check_message_sending(res, "bad_option_message");

    choices
}

pub async fn parse_choices<T: IntoIterator>(
    ctx: &Context,
    msg: &Message,
    arg_info: ArgumentInfo<'_>,
    choices: T,
) -> Result<T::Item, ArgumentParseError>
where
    T::Item: Display + Hash + Eq + FromStr,
{
    let ArgumentInfo { args, arg_pos, args_needed } = arg_info;

    match args.parse::<T::Item>() {
        Ok(arg) => {
            args.advance();

            Ok(arg)
        }
        Err(error) => {
            if let ArgError::Eos = error {
                error_util::not_enough_arguments(ctx, msg.channel_id, arg_pos - 1, args_needed).await;

                Err(ArgumentParseError::NotEnoughArguments(NotEnoughArgumentsError::new(
                    args_needed,
                    arg_pos - 1,
                )))
            } else {
                let options = bad_option_message(ctx, msg, arg_pos, choices.into_iter()).await;
                let current_arg = args
                    .current()
                    .expect("The current argument doesn't exist. This should never happen here.")
                    .to_owned();

                Err(ArgumentParseError::BadOption(BadOptionError::new(arg_pos, current_arg, options)))
            }
        }
    }
}

async fn id_argument_to_role<T: AsRef<Cache>>(
    cache: T,
    arg_pos: usize,
    arg: &str,
    guild_id: impl Into<GuildId>,
    role_id: impl Into<RoleId>,
) -> Result<RoleId, ArgumentParseError> {
    return cache
        .as_ref()
        .guild_field(guild_id, |guild| guild.roles.get(&role_id.into()).map(|role| role.id))
        .await
        .flatten()
        .ok_or_else(|| ArgumentParseError::ArgumentConversionError(ArgumentConversionError::new(arg_pos, arg.to_owned(), ConversionType::Role)));
}

pub async fn parse_role(ctx: &Context, msg: &Message, arg_info: ArgumentInfo<'_>) -> Result<RoleId, ArgumentParseError> {
    let cache = &ctx.cache;
    let guild_id = msg.guild_id.unwrap();
    let ArgumentInfo { args, arg_pos, args_needed } = arg_info;

    match args.parse::<u64>() {
        Ok(user_id) => {
            if let Ok(role_id) = id_argument_to_role(cache, arg_pos, args.current().unwrap(), guild_id, user_id).await {
                args.advance();

                return Ok(role_id);
            }
        }
        Err(error) => {
            if let ArgError::Eos = error {
                error_util::not_enough_arguments(ctx, msg.channel_id, arg_pos - 1, args_needed).await;

                return Err(ArgumentParseError::NotEnoughArguments(NotEnoughArgumentsError::new(
                    args_needed,
                    arg_pos - 1,
                )));
            }
        }
    }

    let arg = args.current().unwrap();

    if let Some(user_id) = parse_role_mention(arg) {
        if let Ok(role_id) = id_argument_to_role(cache, arg_pos, arg, guild_id, user_id).await {
            args.advance();

            return Ok(role_id);
        }
    }

    let msg_str = format!("Invalid argument #{}. Could not find any role with that ID.", arg_pos);

    send_message(ctx, msg.channel_id, msg_str, "parse_role").await;

    Err(ArgumentParseError::ArgumentConversionError(ArgumentConversionError::new(
        arg_pos,
        arg.to_owned(),
        ConversionType::Role,
    )))
}

fn check_message_sending(res: SerenityResult<Message>, function_name: &str) {
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
        .await
        .flatten()
}
