mod forvo;

use futures::future::join_all;
use futures::stream;
use futures::StreamExt;
use log::debug;
use log::error;
use serenity::client::Context;
use serenity::framework::standard::macros::command;
use serenity::framework::standard::macros::group;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::channel::Message;
use strum::IntoEnumIterator;
use util::ArgumentInfo;

use crate::commands;
use crate::commands::language::forvo::Country;
use crate::commands::language::forvo::ForvoError;

use self::forvo::ForvoResult;

use super::error_util;
use super::error_util::error::NotEnoughArgumentsError;
use super::util;

async fn parse_term(ctx: &Context, msg: &Message, args: &mut Args) -> Result<String, NotEnoughArgumentsError> {
    match args.current() {
        Some(arg) => Ok(urlencoding::encode(arg)),
        None => {
            error_util::not_enough_arguments(ctx, msg.channel_id, 0, 1).await;

            Err(NotEnoughArgumentsError::new(1, 0))
        }
    }
}

fn get_pronounce_message(term: &str, country: Country, requested_country: Option<Country>) -> String {
    match requested_country.filter(|&c| c != country) {
        Some(_) => {
            format!(
            "Here is the pronunciation of ``{term}``. The pronunciation from the country closest in terms of accent to the requested country is {country}."
        )
        }
        _ => format!("Here is the pronunciation of ``{term}``. Country: {country}."),
    }
}

async fn send_forvo_recording(ctx: &Context, msg: &Message, term: &str, country: Country, data: &[u8], requested_country: Option<Country>) {
    let result = msg
        .channel_id
        .send_message(&ctx.http, |msg| {
            msg.content(get_pronounce_message(term, country, requested_country));
            msg.add_file((data, "forvo.mp3"))
        })
        .await;

    if let Err(err) = result {
        debug!("Couldn't send forvo message due to error: {:?}", err)
    }
}

fn handle_recording_error<T>(recording: ForvoResult<T>) -> ForvoResult<T> {
    if let Err(err) = &recording {
        error!("{err}");
    }

    recording
}

#[command]
#[bucket("intense")]
#[description("Fetches the pronunciation of something given an optional country of origin as a flag.")]
#[usage("<TERM> [COUNTRY FLAG]")]
#[example("pollo")]
async fn pronounce(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    args.quoted();

    let term = parse_term(ctx, msg, &mut args).await?; // Word to pronounce

    args.advance();

    let requested_country = if args.remaining() >= 1 {
        Some(commands::parse_choices(ctx, msg, ArgumentInfo::new(&mut args, 1, 2), Country::iter()).await?)
    } else {
        None
    };

    let data_res = forvo::fetch_pronunciation(term.as_str(), requested_country).await;
    let info = format!("The term that caused this error was: {term}");
    let pronunciation_data = handle_recording_error(data_res)?;
    let mut recording_futures = Vec::new();

    for recording_data_res in pronunciation_data {
        let res = recording_data_res;

        match res {
            Err(err @ ForvoError::InvalidMatchedCountry(_)) => debug!("{err} -- {info}"),
            Err(err) => error!("{err} -- {info}"),
            Ok(recording) => recording_futures.push(recording),
        }
    }

    if recording_futures.is_empty() {
        util::send_message(ctx, msg.channel_id, "No pronunciation found for the given term.", "pronounce").await;

        return Ok(());
    }

    stream::iter(join_all(recording_futures.iter_mut().map(|r| r.get_recording())).await)
        .filter_map(|r| async { handle_recording_error(r).ok() })
        .for_each_concurrent(None, |(data, country, term)| async move {
            send_forvo_recording(ctx, msg, term, country, data, requested_country).await;
        })
        .await;

    Ok(())
}

#[group]
#[commands(pronounce)]
struct Language;
