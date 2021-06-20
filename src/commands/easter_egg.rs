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
        if person != "f83a5cd5f2e7da713641b6b120502d4d4" && person != "f4bfa4dfcfc4c893e1ceb9bcc2b761ced" {
            msg_to_send = format!("f7a4195263fdc660a66289ae92ebb281ffe39736b1d1ae9fde992c6c1053ab3fc", person);
        } else {
            let msg = "f3bacbc159bb229f61597fc57d29dfff2";
            util::send_message(context, &message.channel_id, msg, "f939b772cfc5408b5a2ea435b558afca0").await;

            return Ok(());
        }
    } else {
        error_util::not_enough_arguments(context, &message.channel_id, 0, 1).await;

        return Ok(());
    }

    util::send_message(context, &message.channel_id, msg_to_send, "f939b772cfc5408b5a2ea435b558afca0").await;

    Ok(())
}

#[group]
#[help_available(false)]
#[commands(f939b772cfc5408b5a2ea435b558afca0)]

struct EasterEgg;
