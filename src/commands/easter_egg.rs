use burdbot::obfuscated_command;
use serenity::client::Context;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::channel::Message;

use super::{error_util, util};

#[obfuscated_command]
#[only_in("guilds")]
#[bucket("default")]
async fn f939b772cfc5408b5a2ea435b558afca0(context: &Context, message: &Message, mut args: Args) -> CommandResult {
    let msg_to_send: String;

    if let Ok(person) = args.single::<String>() {
        if person != "@everyone" && person != "@here" {
            msg_to_send = format!("Alta facha tiene el {}", person);
        } else {
            util::send_message(context, &message.channel_id, "Nice try.", "chamuyar").await;

            return Ok(());
        }
    } else {
        error_util::not_enough_arguments(context, &message.channel_id, 0, 1).await;

        return Ok(());
    }

    util::send_message(context, &message.channel_id, msg_to_send, "chamuyar").await;

    Ok(())
}

#[group]
#[help_available(false)]
#[commands(f939b772cfc5408b5a2ea435b558afca0)]

struct EasterEgg;
