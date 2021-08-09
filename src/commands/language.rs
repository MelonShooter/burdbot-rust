mod forvo;

use serenity::client::Context;
use serenity::framework::standard::macros::command;
use serenity::framework::standard::macros::group;
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::channel::Message;

use super::util;

#[command]
#[bucket("intense")]
#[description("Fetches the pronunciation of something given an optional country of origin as a flag.")]
#[usage("<TERM> [COUNTRY FLAG]")]
#[example("pollo")]
async fn pronounce(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    args.quoted();

    let pronunciation_data = forvo::fetch_pronunciation(ctx, msg, &mut args).await?;

    match &pronunciation_data[..] {
        &[None, None] => util::send_message(ctx, &msg.channel_id, "No pronunciation found for the given term.", "pronounce").await,
        _ => {
            for pronunciation in pronunciation_data {
                match pronunciation {
                    Some(data) => {
                        let recording = data.recording.clone();

                        msg.channel_id
                            .send_message(&ctx.http, |msg| {
                                msg.content(data.message);
                                msg.add_file((&recording[..], "forvo.mp3"))
                            })
                            .await?;
                    }
                    None => continue,
                };
            }
        }
    }

    Ok(())
}

#[group]
#[commands(pronounce)]
struct Language;
