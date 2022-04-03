mod error;

pub use error::*;

use lazy_static::lazy_static;
use regex::Regex;
use serenity::client::{Cache, Context};
use serenity::framework::standard::{ArgError, Args};
use serenity::http::Http;
use serenity::model::channel::Message;
use serenity::model::guild::Member;
use serenity::model::id::{GuildId, RoleId, UserId};
use serenity::utils::Colour;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::ops::Deref;
use std::str::FromStr;
use strum::EnumProperty;
use strum_macros::EnumProperty;
use thiserror::Error;

use crate::util;

pub type Result<T> = std::result::Result<T, ArgumentParseError>;

#[derive(Error, Debug)]
pub enum ArgumentParseError {
    #[error("{0}")]
    OutOfBounds(#[from] ArgumentOutOfBoundsError),
    #[error("{0}")]
    NotEnoughArguments(#[from] NotEnoughArgumentsError),
    #[error("{0}")]
    ArgumentConversionError(#[from] ArgumentConversionError),
    #[error("{0}")]
    BadOption(#[from] BadOptionError),
}

#[derive(Error, Debug, Clone)]
#[error("Invalid choice in argument #{arg_pos}. Choices are {choices}. The argument provided was {provided_choice}")]
pub struct BadOptionError {
    pub arg_pos: usize,
    pub provided_choice: String,
    pub choices: String,
}

impl BadOptionError {
    pub fn new(arg_pos: usize, provided_choice: String, choices: String) -> Self {
        Self {
            arg_pos,
            provided_choice,
            choices,
        }
    }
}

#[derive(Error, Debug, Copy, Clone)]
#[error("Not enough arguments provided. At least {min_args} arg(s) is/are needed. {args_provided} was/were provided.")]
pub struct NotEnoughArgumentsError {
    pub min_args: usize,
    pub args_provided: usize,
}

impl NotEnoughArgumentsError {
    pub fn new(min_args: usize, args_provided: usize) -> Self {
        Self { min_args, args_provided }
    }
}

#[derive(Error, Debug, Copy, Clone)]
#[error("Argument #{arg_pos} is out of bounds. The range (inclusive) for this argument is {lower} to {upper}. The number provided was {arg}.")]
pub struct ArgumentOutOfBoundsError {
    pub lower: i64,
    pub upper: i64,
    pub arg: i64,
    pub arg_pos: usize,
}

impl ArgumentOutOfBoundsError {
    pub fn new(lower: i64, upper: i64, arg: i64, arg_pos: usize) -> Self {
        Self { lower, upper, arg, arg_pos }
    }
}

const CONVERSION_NO_INFO: &str = "Conversions should always have an info property";

#[derive(Error, Debug, Clone)]
#[error("Argument #{arg_pos} could not be converted to a {conversion_type}. {} The argument provided was {arg}.", conversion_type.get_str("info").expect(CONVERSION_NO_INFO))]
pub struct ArgumentConversionError {
    pub arg_pos: usize,
    pub arg: String,
    pub conversion_type: ConversionType,
}

impl ArgumentConversionError {
    pub fn new(arg_pos: usize, arg: String, conversion_type: ConversionType) -> Self {
        Self {
            arg_pos,
            arg,
            conversion_type,
        }
    }
}

// TODO: UPDATE THIS TO TAKE ADVANTAGE OF FROM (use ?) AND NEW DISPLAY IMPLS EVERYWHERE
// TODO: Add different conversion types
#[derive(strum_macros::Display, Debug, EnumProperty, Copy, Clone)]
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

pub async fn parse_bounded_arg(ctx: impl AsRef<Http>, msg: &Message, arg_info: BoundedArgumentInfo<'_>) -> Result<i64> {
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
                check_within_range(ctx, msg.channel_id, month_number, arg_pos, start, end).await;

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
                not_enough_arguments(ctx, msg.channel_id, arg_pos - 1, args_needed).await;

                Err(ArgumentParseError::NotEnoughArguments(NotEnoughArgumentsError::new(
                    args_needed,
                    arg_pos - 1,
                )))
            } else {
                // Must be a parse error.
                check_within_range(ctx, msg.channel_id, args.current().unwrap(), arg_pos, start, end).await;

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
) -> Result<Member> {
    return cache
        .as_ref()
        .member(guild_id, user_id)
        .await
        .ok_or_else(|| ArgumentConversionError::new(arg_pos, arg.to_owned(), ConversionType::Member).into());
}

pub async fn parse_member(ctx: &Context, msg: &Message, arg_info: ArgumentInfo<'_>) -> Result<Member> {
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
                not_enough_arguments(ctx, msg.channel_id, arg_pos - 1, args_needed).await;

                return Err(ArgumentParseError::NotEnoughArguments(NotEnoughArgumentsError::new(
                    args_needed,
                    arg_pos - 1,
                )));
            }
        }
    }

    // TODO: find way to do it by message ID and/or fuzzy matching numbers?

    let arg = args.current().unwrap();

    if let Some(user_id) = parse_user_mention(arg) {
        if let Ok(member) = id_argument_to_member(cache, arg_pos, arg, guild_id, user_id).await {
            args.advance();

            return Ok(member);
        }
    }

    let msg_str = format!("Invalid argument #{}. Could not find any user with that ID or tag.", arg_pos);

    util::send_message(ctx, msg.channel_id, msg_str, "parse_member").await;

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

    util::check_message_sending(res, "bad_option_message");

    choices
}

pub async fn parse_choices<T: IntoIterator>(ctx: &Context, msg: &Message, arg_info: ArgumentInfo<'_>, choices: T) -> Result<T::Item>
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
                not_enough_arguments(ctx, msg.channel_id, arg_pos - 1, args_needed).await;

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
) -> Result<RoleId> {
    return cache
        .as_ref()
        .guild_field(guild_id, |guild| guild.roles.get(&role_id.into()).map(|role| role.id))
        .await
        .flatten()
        .ok_or_else(|| ArgumentParseError::ArgumentConversionError(ArgumentConversionError::new(arg_pos, arg.to_owned(), ConversionType::Role)));
}

pub async fn parse_role(ctx: &Context, msg: &Message, arg_info: ArgumentInfo<'_>) -> Result<RoleId> {
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
                not_enough_arguments(ctx, msg.channel_id, arg_pos - 1, args_needed).await;

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

    util::send_message(ctx, msg.channel_id, msg_str, "parse_role").await;

    Err(ArgumentParseError::ArgumentConversionError(ArgumentConversionError::new(
        arg_pos,
        arg.to_owned(),
        ConversionType::Role,
    )))
}
