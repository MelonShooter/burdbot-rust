use serenity::client::Context;
use serenity::framework::standard::macros::{check, command, group};
use serenity::framework::standard::{Args, CommandError, CommandResult, Reason};
use serenity::model::channel::Message;
use serenity::model::id::{ChannelId, RoleId};
use serenity::utils::Color;

use super::{util, ArgumentInfo};

async fn banfromchannel<'a>(ctx: &Context, msg: &Message, mut args: Args, role_id: &RoleId, ch_name: &'a str) -> Result<String, CommandError> {
    let mut target = util::parse_member(ctx, msg, ArgumentInfo::new(&mut args, 1, 1)).await?;
    let target_name = target.user.name.clone();
    let target_id = target.user.id;

    Ok(if target.roles.contains(role_id) {
        format!("{} ({}) already is banned from the {} channel(s).", target_name, target_id, ch_name)
    } else {
        match target.add_role(&ctx, role_id).await {
            Ok(_) => {
                ChannelId::from(873845572975603792)
                    .send_message(&ctx, |create_msg| {
                        create_msg.embed(|embed| {
                            embed.color(Color::RED);
                            embed.title("User banned from channel(s).");
                            embed.description(format!(
                                "{} ({}) banned {} ({}) from the {} channel(s).",
                                msg.author.name, msg.author.id, target_name, target_id, ch_name
                            ))
                        })
                    })
                    .await?;

                format!("Successfully banned {} ({}) from the {} channel(s).", target_name, target_id, ch_name)
            }
            Err(_) => format!(
                "Failed to ban {} ({}) from the {} channel(s). Check that the user exists \
                and that the bot has the Manage Roles permission.",
                target_name, target_id, ch_name
            ),
        }
    })
}

async fn unbanfromchannel<'a>(ctx: &Context, msg: &Message, mut args: Args, role_id: &RoleId, ch_name: &'a str) -> Result<String, CommandError> {
    let mut target = util::parse_member(ctx, msg, ArgumentInfo::new(&mut args, 1, 1)).await?;
    let target_name = target.user.name.clone();
    let target_id = target.user.id;

    Ok(if !target.roles.contains(role_id) {
        format!(
            "{} ({}) was not banned from the {} channel(s) in the first place.",
            target_name, target_id, ch_name
        )
    } else {
        match target.remove_role(&ctx, role_id).await {
            Ok(_) => {
                ChannelId::from(873845572975603792)
                    .send_message(&ctx, |create_msg| {
                        create_msg.embed(|embed| {
                            embed.color(Color::DARK_GREEN);
                            embed.title("User unbanned from channel(s)");
                            embed.description(format!(
                                "{} ({}) unbanned {} ({}) from the {} channel(s).",
                                msg.author.name, msg.author.id, target_name, target_id, ch_name
                            ))
                        })
                    })
                    .await?;

                format!("Successfully unbanned {} ({}) from the {} channels.", target_name, target_id, ch_name)
            }
            Err(_) => format!(
                "Failed to unban {} ({}) from the {} channel(s). Check that the user exists \
                and that the bot has the Manage Roles permission.",
                target_name, target_id, ch_name
            ),
        }
    })
}

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
async fn banfrommemes(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let message_to_send = banfromchannel(ctx, msg, args, &RoleId::from(863822767702409216), "memes").await?;

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
async fn unbanfrommemes(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let message_to_send = unbanfromchannel(ctx, msg, args, &RoleId::from(863822767702409216), "memes").await?;

    util::send_message(ctx, &msg.channel_id, message_to_send, "unbanfrommemes").await;

    Ok(())
}

#[group]
#[commands(banfrommemes, unbanfrommemes)]
struct Custom;
