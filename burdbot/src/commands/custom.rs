use crate::argument_parser::{self, ArgumentInfo};
use crate::commands::error_util;
use crate::image_checker::{ImageChecker, MessageImages};
use crate::spanish_english::{
    IS_SERVER_HELPER_OR_ABOVE_CHECK, SPANISH_ENGLISH_SERVER_ID, SPANISH_ENGLISH_STAFF_CHANNEL_ID,
    SPANISH_ENGLISH_STAFF_ROLE,
};
use crate::util::{self, get_ids_from_msg_link};

use chrono::TimeDelta;
use log::{error, info};
use serenity::all::{
    CreateAllowedMentions, CreateEmbed, CreateMessage, EMBED_MAX_COUNT, GuildId, Mentionable,
    Permissions, Timestamp,
};
use serenity::client::Context;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::channel::Message;
use serenity::model::colour::Color;
use serenity::model::id::{ChannelId, RoleId};
use strum_macros::{Display, FromRepr};

async fn banfromchannel(
    ctx: &Context, msg: &Message, mut args: Args, role_id: RoleId, ch_name: &str,
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
    ctx: &Context, msg: &Message, mut args: Args, role_id: RoleId, ch_name: &str,
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

/// Validates an image link by parsing and checking that:
/// - The linked message isn't from the provided guild ID
/// - The message link wasn't valid
async fn validate_image_link(
    ctx: &Context, curr_channel: ChannelId, link: &str, from_guild: GuildId,
) -> Option<Message> {
    // Parse link
    let Some((guild_id, channel_id, msg_id)) = get_ids_from_msg_link(link) else {
        util::send_message(ctx, curr_channel, "Invalid message link", "validate_image_link").await;
        return None;
    };

    if from_guild != guild_id {
        util::send_message(
            ctx, curr_channel, "Message link must be from this server", "validate_image_link",
        )
        .await;
        return None;
    }

    let msg = channel_id.message(ctx, msg_id).await.ok();

    if msg.is_none() {
        util::send_message(
            ctx,
            curr_channel,
            "Couldn't fetch message. Check message link is valid and I have \
             permission to access it",
            "validate_image_link",
        )
        .await;
    }

    msg
}

static TIMEOUT_DURATION: TimeDelta = TimeDelta::days(7);
static IMAGE_HASHER: ImageChecker<blake3::Hasher> = ImageChecker::new();
static IMAGE_HASHER_TYPE: HashType = HashType::Blake3;

#[derive(Display, FromRepr, Copy, Clone, PartialEq, Eq)]
pub enum HashType {
    #[strum(to_string = "BLAKE3")]
    Blake3 = 0,
}

/// Times out the user for 7 days and
/// deletes the message. If it's the Spanish-English discord server,
/// or a test server, then also notify in a channel.
/// Prints info trace and returns if no perms to time out, or delete
///
/// Must provide the offending message and the guild ID
async fn time_out_delete_and_notify(
    ctx: &Context, msg: &Message, banned_img_link: &str, img_msg_link_db_ref: String,
    guild_id: GuildId,
) {
    let Ok(mut member) = guild_id.member(ctx, msg.author.id).await else {
        error!(
            "Failed to get member when trying to time out and delete their msgs. Guild ID \
                {guild_id}. user id: {}",
            msg.author.id
        );
        return;
    };

    // Only delete msg if we have permission to timeout the member
    // Because it could be staff who's trying to paste the image
    let timeout_res = member
        .disable_communication_until_datetime(
            ctx,
            Timestamp::now().checked_add_signed(TIMEOUT_DURATION).unwrap().into(),
        )
        .await;

    let timeout_str = format!("Timed out user for {} days", TIMEOUT_DURATION.num_days());
    let could_timeout = if let Err(e) = timeout_res {
        info!("Tried to time out {} and failed. Likely permission issue: {e:?}", member.user.id);
        "Failed to timeout"
    } else {
        timeout_str.as_str()
    };

    let embed = CreateEmbed::new()
        .color(Color::RED)
        .title("Banned Image Detected")
        .field("Sent by", format!("{} {}", msg.author.mention(), msg.author.name), true)
        .field("In", msg.channel_id.mention().to_string(), true)
        .field("Deleted Image", format!("[Image]({banned_img_link})"), false)
        .field("Link from database", format!("[Message]({img_msg_link_db_ref})"), true)
        .field("Action taken", could_timeout, false)
        .timestamp(Timestamp::now());

    let (ch_id, response) = if guild_id == SPANISH_ENGLISH_SERVER_ID {
        let staff_notification = CreateMessage::new()
            .embed(embed)
            .content(SPANISH_ENGLISH_STAFF_ROLE.mention().to_string());
        (SPANISH_ENGLISH_STAFF_CHANNEL_ID, staff_notification)
    } else {
        (
            msg.channel_id,
            CreateMessage::new()
                .embed(embed)
                .reference_message(msg)
                .allowed_mentions(CreateAllowedMentions::new()), // Don't ping with reply
        )
    };

    if let Err(e) = ch_id.send_message(ctx, response).await {
        error!("Error sending banned image resp to server {guild_id} channel {ch_id}. Err: {e}");
    }

    if let Err(e) = msg.delete(ctx).await {
        util::send_message(
            ctx,
            ch_id,
            format!("Failed to delete message. Err: {e}"),
            "time_out_delete_and_notify",
        )
        .await;
    }

    info!("Deleted banned image in server: {} from {}", guild_id, member.user.id);
}

/* If user has any of these permission, they are exempted from banned images */
const PERM_EXEMPTION: Permissions =
    Permissions::MANAGE_MESSAGES.union(Permissions::MODERATE_MEMBERS);

pub async fn on_message_receive(ctx: &Context, msg: &Message) {
    let Some(guild_id) = msg.guild_id else {
        return;
    };

    if msg.author.bot {
        return;
    }

    let images = MessageImages(msg);

    if let Some(perms) = msg.author_permissions(ctx) {
        // Means this user has at least one permission from PERM_EXEMPTION, so return
        // and don't run anything
        if !perms.intersection(PERM_EXEMPTION).is_empty() {
            return;
        }
    }

    for image @ (img_link, ..) in images.to_vec() {
        match IMAGE_HASHER.check_image(guild_id, image).await {
            Ok(Some(db_link_ref)) => {
                time_out_delete_and_notify(ctx, msg, img_link, db_link_ref, guild_id).await;
                break;
            },
            Err(e) => error!("Internal error checking for banned image: {e:?}"),
            _ => (),
        }
    }
}

#[command]
#[checks(is_server_helper_or_above)]
#[only_in("guilds")]
#[usage("<link to message with one image> <description>")]
#[example(
    "https://discord.com/channels/243838819743432704/1386127080827392155/1386127084732289075 This is my description"
)]
#[description(
    "Bans an image given a link to the message with the image and a description. The link should lead to a \
     message in this server. It would be preferable to just choose an image already in the logs. \
     You're exempted if you have permission to time out or manage messages."
)]
async fn banimage(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    if args.len() < 2 {
        util::send_message(
            ctx, msg.channel_id, "Provide a message link and description", "banimage",
        )
        .await;
        return Ok(());
    }

    let validated =
        validate_image_link(ctx, msg.channel_id, args.current().unwrap(), msg.guild_id.unwrap())
            .await;
    let Some(target_msg) = validated else {
        return Ok(());
    };

    args.advance();
    let desc = args.remains().unwrap();

    match IMAGE_HASHER
        .add_image(desc, msg.guild_id.unwrap(), &target_msg, IMAGE_HASHER_TYPE as u16)
        .await
    {
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

    if let Ok(images) = banned {
        if images.is_empty() {
            let reply = CreateMessage::new()
                .add_embed(CreateEmbed::new().color(Color::RED).title("No banned images found"));

            if let Err(e) = msg.channel_id.send_message(ctx, reply).await {
                info!(
                    "Couldn't send message to {}. Likely have read perms but not write. error: {e:?}",
                    msg.channel_id
                );
            }

            return Ok(());
        }

        for image_chunk in images.chunks(EMBED_MAX_COUNT) {
            let mut reply = CreateMessage::new();

            for image in image_chunk {
                let msg_link_parts = get_ids_from_msg_link(&image.link_ref);
                let hash_type = HashType::from_repr(image.hash_type as usize).unwrap().to_string();
                let mut embed = CreateEmbed::new()
                    .color(Color::DARK_GREEN)
                    .title(image.description.to_string())
                    .field("Link", image.link_ref.clone(), true)
                    .field("Dimensions", format!("{}x{}", image.width, image.height), true)
                    .field(format!("{hash_type} hash"), &image.hash_hex, false);

                // Set thumbnail for the embed if available. If not, it may have been deleted
                if let Some((_, ch_id, msg_id)) = msg_link_parts {
                    if let Ok(msg) = ch_id.message(ctx, msg_id).await {
                        if let Some(&(url, ..)) = MessageImages(&msg).to_vec().first() {
                            embed = embed.thumbnail(url)
                        }
                    }
                }

                reply = reply.add_embed(embed);
            }

            if let Err(e) = msg.channel_id.send_message(ctx, reply).await {
                info!(
                    "Couldn't send message to {}. Likely have read perms but not write. error: {e:?}",
                    msg.channel_id
                );

                return Ok(());
            }
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
