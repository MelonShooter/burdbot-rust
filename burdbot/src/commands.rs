mod administrative;
mod birthday;
mod custom;
mod easter_egg;
mod error_util;
mod language;

pub mod vocaroo;

pub use administrative::ADMINISTRATIVE_GROUP;
pub use birthday::BirthdayInfoConfirmation;
pub use birthday::BIRTHDAY_GROUP;
pub use birthday::MONTH_TO_DAYS;
pub use birthday::MONTH_TO_NAME;
pub use custom::CUSTOM_GROUP;
pub use easter_egg::EASTEREGG_GROUP;
pub use language::LANGUAGE_GROUP;
pub use vocaroo::VOCAROO_GROUP;

use std::collections::HashSet;

use serenity::client::Context;
use serenity::framework::standard::help_commands;
use serenity::framework::standard::macros::help;
use serenity::framework::standard::Args;
use serenity::framework::standard::CommandGroup;
use serenity::framework::standard::CommandResult;
use serenity::framework::standard::HelpOptions;
use serenity::model::channel::Message;
use serenity::model::id::UserId;

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
    help_commands::with_embeds(context, msg, args, help_options, groups, owners).await?;

    Ok(())
}
