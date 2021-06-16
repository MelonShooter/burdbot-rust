use std::fmt::Debug;
use std::fmt::Display;
use std::str::FromStr;

use serenity::client::Cache;
use serenity::client::Context;
use serenity::framework::standard::{ArgError, Args};
use serenity::http::Http;
use serenity::model::channel::Message;
use serenity::model::id::ChannelId;
use serenity::model::prelude::User;
use serenity::prelude::ModelError;
use serenity::Error;

use log::error;

use super::error_util;
use super::error_util::error::{ArgumentConversionError, ArgumentOutOfBoundsError, ArgumentParseErrorType, NotEnoughArgumentsError};

pub mod user_search_engine;

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

pub async fn parse_user(ctx: &Context, msg: &Message, args: &mut Args) -> Result<u64, ArgumentParseErrorType<u32>> {
    // Make method to parse user (mention, user id, and also by name and discriminator)
    // GO back to line 86 of birthday.rs to finish up
    Ok(3)
}

pub async fn send_message(ctx: impl AsRef<Http>, ch: &ChannelId, msg: impl Display, function_name: &str) {
    if let Err(error) = ch.say(ctx, msg).await {
        if let Error::Model(ModelError::MessageTooLong(_)) = error {
            error!("{}() message too long! This shouldn't ever happen.", function_name);
        }
    }
}

pub async fn get_user(cache: &Cache, http: &Http, user_id: u64) -> Option<User> {
    if let Some(user) = cache.user(user_id).await {
        return Some(user);
    }

    match http.get_user(user_id).await {
        Ok(user) => Some(user),
        Err(_) => None,
    }
}