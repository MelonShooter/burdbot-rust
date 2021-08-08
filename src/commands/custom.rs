use serenity::client::Context;
use serenity::framework::standard::macros::{check, command, group};
use serenity::framework::standard::{Args, CommandResult, Reason};
use serenity::model::channel::Message;
use serenity::model::id::{ChannelId, RoleId};
use serenity::utils::Color;

use super::{util, ArgumentInfo};

#[check]
async fn is_server_helper_or_above(ctx: &Context, msg: &Message) -> Result<(), Reason> {
    let author = match msg.member(&ctx).await {
        Ok(member) => member,
        Err(_) => return Err(Reason::Unknown),
    };

    match author
        .roles
        .iter()
        .any(|id| id.0 == 243854949522472971 || id.0 == 258806166770024449 || id.0 == 258819531193974784)
    {
        true => Ok(()),
        false => Err(Reason::Log("User is lower than a server helper.".to_owned())),
    }
}

#[command]
#[checks(is_server_helper_or_above)]
#[only_in("guilds")]
#[usage("<USER>")]
#[example("367538590520967181")]
#[example("DELIBURD#7741")]
#[description("Ban a user from the memes channel.")]
async fn banfrommemes(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let mut target = util::parse_member(ctx, msg, ArgumentInfo::new(&mut args, 1, 1)).await?;
    let target_name = target.user.name.clone();
    let target_id = target.user.id;
    let message_to_send;

    if target.roles.contains(&RoleId::from(863822767702409216)) {
        message_to_send = format!("{} ({}) already is banned from memes.", target_name, target_id);
    } else {
        message_to_send = match target.add_role(&ctx, 863822767702409216).await {
            Ok(_) => {
                ChannelId::from(873845572975603792)
                    .send_message(&ctx, |create_msg| {
                        create_msg.embed(|embed| {
                            embed.color(Color::RED);
                            embed.title("User Banned From Memes");
                            embed.description(format!(
                                "{} ({}) banned {} ({}) from the memes channel.",
                                msg.author.name, msg.author.id, target_name, target_id
                            ))
                        })
                    })
                    .await?;

                format!("Successfully banned {} ({}) from the memes channel.", target_name, target_id)
            }
            Err(_) => format!(
                "Failed to ban {} ({}) from the memes channel. Check that the user exists \
                and that the bot has the Manage Roles permission.",
                target_name, target_id
            ),
        };
    }

    util::send_message(ctx, &msg.channel_id, message_to_send, "banfrommemes").await;

    Ok(())
}

#[command]
#[checks(is_server_helper_or_above)]
#[only_in("guilds")]
#[usage("<USER>")]
#[example("367538590520967181")]
#[example("DELIBURD#7741")]
#[description("Unban a user from the memes channel.")]
async fn unbanfrommemes(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let mut target = util::parse_member(ctx, msg, ArgumentInfo::new(&mut args, 1, 1)).await?;
    let target_name = target.user.name.clone();
    let target_id = target.user.id;
    let message_to_send;

    if !target.roles.contains(&RoleId::from(863822767702409216)) {
        message_to_send = format!("{} ({}) was not banned from memes in the first place.", target_name, target_id);
    } else {
        message_to_send = match target.remove_role(&ctx, 863822767702409216).await {
            Ok(_) => {
                ChannelId::from(873845572975603792)
                    .send_message(&ctx, |create_msg| {
                        create_msg.embed(|embed| {
                            embed.color(Color::DARK_GREEN);
                            embed.title("User Unbanned From Memes");
                            embed.description(format!(
                                "{} ({}) unbanned {} ({}) from the memes channel.",
                                msg.author.name, msg.author.id, target_name, target_id
                            ))
                        })
                    })
                    .await?;

                format!("Successfully unbanned {} ({}) from the memes channel.", target_name, target_id)
            }
            Err(_) => format!(
                "Failed to unban {} ({}) from the memes channel. Check that the user exists \
                and that the bot has the Manage Roles permission.",
                target_name, target_id
            ),
        };
    }

    util::send_message(ctx, &msg.channel_id, message_to_send, "unbanfrommemes").await;

    Ok(())
}

#[group]
#[commands(banfrommemes, unbanfrommemes)]
struct Custom;
