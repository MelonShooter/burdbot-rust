use serenity::client::Context;
use serenity::model::id::ChannelId;

use crate::util;

pub async fn generic_fail(ctx: &Context, ch: ChannelId) {
    util::send_message(&ctx.http, ch, "Something went wrong. The owner of the bot has been notified of this.", "generic_fail").await;
}

/*pub async fn unknown_command_message(ctx: impl AsRef<Http>, ch: ChannelId) {
    let unknown_command_message = "Unknown command. Type the help command to get the list of commands.";

    util::send_message(ctx, ch, unknown_command_message, "unknown_command_message").await;
}*/
