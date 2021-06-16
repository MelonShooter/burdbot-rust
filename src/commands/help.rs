use serenity::client::Context;
use serenity::framework::standard::macros::help;
use serenity::framework::standard::{help_commands, Args, CommandGroup, CommandResult, HelpOptions};
use serenity::model::channel::Message;
use serenity::model::id::UserId;
use std::collections::HashSet;

#[help]
#[strikethrough_commands_tip_in_dm("")]
#[strikethrough_commands_tip_in_guild("")]
#[lacking_role("Hide")]
#[lacking_ownership("Hide")]
#[lacking_permissions("Hide")]
#[lacking_conditions("Hide")]
#[wrong_channel("Hide")]
async fn help(
    context: &Context,
    msg: &Message,
    args: Args,
    help_options: &'static HelpOptions,
    groups: &[&'static CommandGroup],
    owners: HashSet<UserId>,
) -> CommandResult {
    help_commands::with_embeds(context, msg, args, help_options, groups, owners).await;

    Ok(())
}
