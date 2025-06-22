use crate::argument_parser::{self, ArgumentInfo};
use crate::commands::error_util;
use crate::image_checker::ImageChecker;
use crate::spanish_english::IS_SERVER_HELPER_OR_ABOVE_CHECK;
use crate::util::{self, get_ids_from_msg_link};

use chrono::Days;
use log::{error, info};
use serenity::all::{CreateEmbed, CreateMessage, GuildId, Member, Timestamp};
use serenity::client::Context;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::channel::Message;
use serenity::model::colour::Color;
use serenity::model::id::{ChannelId, RoleId};
use strum_macros::{Display, FromRepr};

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

// TODO: write message event thingy here

// Validates an image link by parsing and checking it
async fn validate_image_link(
    ctx: &Context,
    curr_channel: ChannelId,
    link: &str,
) -> Option<Message> {
    // Parse link
    let Some((guild_id, channel_id, msg_id)) = get_ids_from_msg_link(link) else {
        util::send_message(ctx, curr_channel, "Invalid message link", "validate_image_link").await;
        return None;
    };

    let target_msg = channel_id.message(ctx, msg_id).await.ok()?;

    if !target_msg.guild_id.is_some_and(|v| v == guild_id) {
        util::send_message(
            ctx,
            curr_channel,
            "Message link must be from this server",
            "validate_image_link",
        )
        .await;
        return None;
    }

    Some(target_msg)
}

static TIMEOUT_DURATION: Days = Days::new(7);
static IMAGE_HASHER: ImageChecker<blake3::Hasher> = ImageChecker::new();
static IMAGE_HASHER_TYPE: HashType = HashType::Blake3;

#[derive(Display, FromRepr, Copy, Clone, PartialEq, Eq)]
pub enum HashType {
    #[strum(to_string = "BLAKE3")]
    Blake3 = 0,
}

async fn time_out_and_delete(ctx: &Context, member: &mut Member, msg: &Message, guild_id: GuildId) {
    // Only delete msg if we have permission to timeout the member
    // Because it could be staff who's trying to paste the image
    let timeout_res = member
        .disable_communication_until_datetime(
            ctx,
            Timestamp::now().checked_add_days(TIMEOUT_DURATION).unwrap().into(),
        )
        .await;

    if let Err(e) = timeout_res {
        info!("Tried to time out {} and failed. Likely permission issue: {e:?}", member.user.id);
        return;
    }

    // At this point, the timeout must've succeeded, so going to try deleting now and then sending out message
    if let Err(e) = msg.delete(ctx).await {
        error!("Failed to delete banned image. error: {e:?}");
        return;
    }

    // TODO: send message out and log somewhere

    info!("Deleted banned image in server: {} from {}", guild_id, member.user.id);
}

pub async fn on_message_receive(ctx: &Context, msg: &Message) {
    let Some(guild_id) = msg.guild_id else {
        return;
    };

    let member = if msg.attachments.len() != 0 {
        match msg.member(ctx).await {
            Ok(m) => m,
            Err(e) => {
                error!("Couldn't get member while checking if they sent banned image: {e:?}");
                return;
            },
        }
    } else {
        return;
    };

    for attachment in &msg.attachments {
        match IMAGE_HASHER.check_image(guild_id, attachment).await {
            Ok(false) => {
                // time_out_and_delete(ctx, &mut member, msg, guild_id).await;

                // Do a dry-run first
                util::send_message(
                    ctx,
                    msg.channel_id,
                    format!("Banned image detected. <&@642782671109488641>"),
                    "on_message_receive",
                )
                .await;
            },
            Err(e) => error!("Internal error checking for banned image: {e:?}"),
            _ => (),
        }
    }
}

#[command]
#[checks(is_server_helper_or_above)]
#[only_in("guilds")]
#[usage("<link to message with one image attachment> <description>")]
#[example(
    "https://discord.com/channels/243838819743432704/1386127080827392155/1386127084732289075 This is my description"
)]
#[description(
    "Bans an image given a link to the message with the image and a description. The link should lead to a \
     message in this server. It would be preferable to just choose an image already in the logs."
)]
async fn banimage(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    if args.len() < 2 {
        util::send_message(
            ctx,
            msg.channel_id,
            "Provide a message link and description",
            "banimage",
        )
        .await;
        return Ok(());
    }

    let Some(target_msg) = validate_image_link(ctx, msg.channel_id, args.current().unwrap()).await
    else {
        return Ok(());
    };

    args.advance();
    let desc = args.remains().unwrap();

    match IMAGE_HASHER.add_image(desc, &target_msg, IMAGE_HASHER_TYPE as u16).await {
        Ok(image_outcome) => {
            util::send_message(ctx, msg.channel_id, image_outcome.to_string(), "banimage").await;
        },
        Err(err) => {
            error_util::generic_fail(ctx, msg.channel_id).await;
            error!("Error banning image: {err:?}");
        },
    };

    Ok(())
}

#[command]
#[checks(is_server_helper_or_above)]
#[only_in("guilds")]
#[usage("<link to message with banned image>")]
#[example(
    "https://discord.com/channels/243838819743432704/1386127080827392155/1386127084732289075"
)]
#[description(
    "Unbans an image given a link to the message with the image. The link should lead to a \
     message in this server."
)]
async fn unbanimage(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    if args.len() != 1 {
        util::send_message(ctx, msg.channel_id, "Provide one message link argument", "banimage")
            .await;
        return Ok(());
    }

    match IMAGE_HASHER.remove_image(msg.guild_id.unwrap(), args.current().unwrap()) {
        Ok(image_outcome) => {
            util::send_message(ctx, msg.channel_id, image_outcome.to_string(), "unbanimage").await;
        },
        Err(err) => {
            error_util::generic_fail(ctx, msg.channel_id).await;
            error!("Error unbanning image: {err:?}");
        },
    };

    Ok(())
}

#[command]
#[checks(is_server_helper_or_above)]
#[only_in("guilds")]
#[description("Lists info on all banned images for the server.")]
async fn bannedimages(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let banned = IMAGE_HASHER.get_images(msg.guild_id.unwrap());
    let mut reply = CreateMessage::new();

    if let Ok(images) = banned {
        for (i, image) in images.into_iter().enumerate() {
            let hash_type = HashType::from_repr(image.hash_type as usize).unwrap().to_string();
            let embed = CreateEmbed::new()
                .color(Color::DARK_GREEN)
                .title(format!("Banned image #{i}"))
                .field("Description", image.description, false)
                .field("Width", image.width.to_string(), false)
                .field("Height", image.height.to_string(), true)
                .field("Hash", image.hash_hex, false)
                .field("Hash Type", hash_type, true)
                .field("Link", image.link_ref, false);

            reply = reply.add_embed(embed);
        }
    } else if let Err(e) = banned {
        error_util::generic_fail(ctx, msg.channel_id).await;
        error!("Error getting banned images: {e:?}");
    }

    Ok(())
}

#[group]
#[commands(banfrommemes, unbanfrommemes, banimage, unbanimage, bannedimages)]
struct Custom;
