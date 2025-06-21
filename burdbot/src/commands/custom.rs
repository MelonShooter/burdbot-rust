use crate::argument_parser::{self, ArgumentInfo};
use crate::spanish_english::IS_SERVER_HELPER_OR_ABOVE_CHECK;
use crate::util;

use serenity::all::{CreateEmbed, CreateMessage};
use serenity::client::Context;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::channel::Message;
use serenity::model::colour::Color;
use serenity::model::id::{ChannelId, RoleId};

async fn banfromchannel(
    ctx: &Context,
    msg: &Message,
    mut args: Args,
    role_id: RoleId,
    ch_name: &str,
) -> CommandResult<String> {
    let target =
        argument_parser::parse_member(ctx, msg, ArgumentInfo::new(&mut args, 1, 1)).await?;
    let target_id = target.user.id;

    Ok(if target.roles.contains(&role_id) {
        format!(
            "{} ({}) already is banned from the {} channel(s).",
            target.user.name.as_str(),
            target_id,
            ch_name
        )
    } else {
        match target.add_role(&ctx, role_id).await {
            Ok(()) => {
                let target_name = target.user.name.as_str();
                let embed = CreateEmbed::new()
                    .color(Color::RED)
                    .title("User banned from channel(s).")
                    .description(format!(
                        "{} ({}) banned {} ({}) from the {} channel(s).",
                        msg.author.name, msg.author.id, target_name, target_id, ch_name
                    ));

                ChannelId::from(873845572975603792)
                    .send_message(&ctx, CreateMessage::new().embed(embed))
                    .await?;

                format!(
                    "Successfully banned {} ({}) from the {} channel(s).",
                    target_name, target_id, ch_name
                )
            },
            Err(_) => format!(
                "Failed to ban {} ({}) from the {} channel(s). Check that the user exists \
                and that the bot has the Manage Roles permission.",
                target.user.name.as_str(),
                target_id,
                ch_name
            ),
        }
    })
}

async fn unbanfromchannel(
    ctx: &Context,
    msg: &Message,
    mut args: Args,
    role_id: RoleId,
    ch_name: &str,
) -> CommandResult<String> {
    let target =
        argument_parser::parse_member(ctx, msg, ArgumentInfo::new(&mut args, 1, 1)).await?;
    let target_id = target.user.id;

    Ok(if target.roles.contains(&role_id) {
        match target.remove_role(&ctx, role_id).await {
            Ok(_) => {
                let target_name = target.user.name.as_str();
                let embed = CreateEmbed::new()
                    .color(Color::DARK_GREEN)
                    .title("User unbanned from channel(s)")
                    .description(format!(
                        "{} ({}) unbanned {} ({}) from the {} channel(s).",
                        msg.author.name, msg.author.id, target_name, target_id, ch_name
                    ));

                ChannelId::from(873845572975603792)
                    .send_message(&ctx, CreateMessage::new().embed(embed))
                    .await?;

                format!(
                    "Successfully unbanned {} ({}) from the {} channels.",
                    target_name, target_id, ch_name
                )
            },
            Err(_) => format!(
                "Failed to unban {} ({}) from the {} channel(s). Check that the user exists \
                and that the bot has the Manage Roles permission.",
                target.user.name.as_str(),
                target_id,
                ch_name
            ),
        }
    } else {
        format!(
            "{} ({}) was not banned from the {} channel(s) in the first place.",
            target.user.name.as_str(),
            target_id,
            ch_name
        )
    })
}

#[command]
#[checks(is_server_helper_or_above)]
#[only_in("guilds")]
#[usage("<USER>")]
#[example("367538590520967181")]
#[example("DELIBURD#7741")]
#[description("Ban a user from the memes channel.")]
async fn banfrommemes(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let message_to_send =
        banfromchannel(ctx, msg, args, RoleId::from(863822767702409216), "memes").await?;

    util::send_message(ctx, msg.channel_id, message_to_send, "banfrommemes").await;

    Ok(())
}

#[command]
#[checks(is_server_helper_or_above)]
#[only_in("guilds")]
#[usage("<USER>")]
#[example("367538590520967181")]
#[example("DELIBURD#7741")]
#[description("Unban a user from the memes channel.")]
async fn unbanfrommemes(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let message_to_send =
        unbanfromchannel(ctx, msg, args, RoleId::from(863822767702409216), "memes").await?;

    util::send_message(ctx, msg.channel_id, message_to_send, "unbanfrommemes").await;

    Ok(())
}

#[group]
#[commands(banfrommemes, unbanfrommemes)]
struct Custom;
