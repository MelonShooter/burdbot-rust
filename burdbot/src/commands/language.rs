use futures::StreamExt;
use futures::future::join_all;
use futures::stream;
use log::debug;
use log::error;
use serenity::all::CreateAttachment;
use serenity::all::CreateMessage;
use serenity::client::Context;
use serenity::framework::standard::macros::command;
use serenity::framework::standard::macros::group;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::channel::Message;
use serenity::model::id::ChannelId;
use strum::IntoEnumIterator;

use crate::argument_parser;
use crate::argument_parser::ArgumentInfo;
use crate::argument_parser::NotEnoughArgumentsError;
use crate::forvo;
use crate::forvo::Country;
use crate::forvo::ForvoError;
use crate::util;

use super::error_util;

async fn parse_term(
    ctx: &Context, msg: &Message, args: &mut Args,
) -> Result<String, NotEnoughArgumentsError> {
    match args.current() {
        Some(arg) => Ok(urlencoding::encode(arg).into_owned()),
        None => {
            argument_parser::not_enough_arguments(ctx, msg.channel_id, 0, 1).await;

            Err(NotEnoughArgumentsError::new(1, 0))
        },
    }
}

fn get_pronounce_message(
    term: &str, country: Country, requested_country: Option<Country>,
) -> String {
    match requested_country.filter(|&c| c != country) {
        Some(_) => {
            format!(
                "Here is the pronunciation of ``{term}``. The pronunciation from the country closest in terms of accent to the requested country is {country}."
            )
        },
        _ => format!("Here is the pronunciation of ``{term}``. Country: {country}."),
    }
}

async fn send_forvo_recording(
    ctx: &Context, msg: &Message, term: &str, country: Country, data: &[u8],
    requested_country: Option<Country>,
) {
    let result = msg
        .channel_id
        .send_message(
            &ctx.http,
            CreateMessage::new()
                .content(get_pronounce_message(term, country, requested_country))
                .add_file(CreateAttachment::bytes(data, "forvo.mp3")),
        )
        .await;

    if let Err(err) = result {
        debug!("Couldn't send forvo message due to error: {:?}", err)
    }
}

async fn handle_recording_error<T>(
    ctx: &Context, ch_id: ChannelId, term: &str, recording: &forvo::Result<T>, is_error: bool,
) {
    if let Err(err) = recording {
        if is_error {
            error!("{err} -- caused by term: {term}.");

            error_util::generic_fail(ctx, ch_id).await;
        } else {
            debug!("{err} -- caused by term: {term}.");
        }
    }
}

async fn handle_recording_error_res<T>(
    ctx: &Context, ch_id: ChannelId, term: &str, recording: forvo::Result<T>, is_error: bool,
) -> forvo::Result<T> {
    handle_recording_error(ctx, ch_id, term, &recording, is_error).await;

    recording
}

#[command]
#[bucket("intense")]
#[description(
    "Fetches the pronunciation of something given an optional country of origin as a flag."
)]
#[usage("<TERM> [COUNTRY FLAG]")]
#[example("pollo")]
async fn pronounce(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    args.quoted();

    let term = parse_term(ctx, msg, &mut args).await?; // Word to pronounce

    args.advance();

    let requested_country = if args.remaining() >= 1 {
        Some(
            argument_parser::parse_choices(
                ctx,
                msg,
                ArgumentInfo::new(&mut args, 1, 2),
                Country::iter(),
            )
            .await?,
        )
    } else {
        None
    };

    let data_res = forvo::fetch_pronunciation(term.as_str(), requested_country).await;
    let pronunciation_data =
        handle_recording_error_res(ctx, msg.channel_id, term.as_str(), data_res, true).await?;
    let mut recording_futures = Vec::new();

    for recording_data_res in pronunciation_data {
        let res = recording_data_res;

        match res {
            err @ Err(ForvoError::InvalidMatchedCountry(_)) => {
                handle_recording_error(ctx, msg.channel_id, term.as_str(), &err, false).await
            },
            Err(_) => handle_recording_error(ctx, msg.channel_id, term.as_str(), &res, true).await,
            Ok(recording) => recording_futures.push(recording),
        }
    }

    if recording_futures.is_empty() {
        util::send_message(
            ctx,
            msg.channel_id,
            "No pronunciation found for the given term.",
            "pronounce",
        )
        .await;

        return Ok(());
    }

    stream::iter(join_all(recording_futures.iter_mut().map(|r| r.get_recording())).await)
        .filter_map(|r| async {
            handle_recording_error_res(ctx, msg.channel_id, term.as_str(), r, true).await.ok()
        })
        .for_each_concurrent(None, |(data, country, term)| async move {
            send_forvo_recording(ctx, msg, term, country, data, requested_country).await;
        })
        .await;

    Ok(())
}

#[group]
#[commands(pronounce)]
struct Language;
