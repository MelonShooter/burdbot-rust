use serenity::client::Context;
use serenity::model::channel::Message;

use crate::commands;

const BOT_PREFIXES: [&str; 5] = ["-", "--", "---", "!", "!!"];

pub async fn on_message_receive(ctx: &Context, message: &Message) {
    do_music_check(ctx, message).await;
}

pub async fn do_music_check(ctx: &Context, message: &Message) {
    let music_channel_id = 263643662808776704u64;
    let channel_id = message.channel_id.0;

    if channel_id != music_channel_id {
        return;
    }

    let content = message.content.as_str();

    for prefix in BOT_PREFIXES {
        if content.starts_with(prefix) {
            let msg_str = "Please put music bot commands in <#247135634265735168> as they do not work here. \
            Por favor, poné los comandos de música en <#247135634265735168>. No funcionan por acá.";

            commands::send_message(ctx, &message.channel_id, msg_str, "on_message_receive").await;

            return;
        }
    }
}
