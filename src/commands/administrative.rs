use lazy_static::lazy_static;
use log::error;
use regex::Regex;
use rusqlite::{Connection, Error};
use serenity::builder::{CreateEmbed, CreateMessage};
use serenity::client::Context;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{Args, CommandError, CommandResult};
use serenity::model::channel::Message;
use serenity::model::guild::Member;
use serenity::model::id::MessageId;
use serenity::utils::Color;

use crate::BURDBOT_DB;

use super::error_util::error::{ArgumentConversionError, ArgumentParseErrorType};
use super::{util, ArgumentInfo};

fn get_message_id_from_link(link: &str) -> u64 {
    lazy_static! {
        static ref MESSAGE_ID_REGEX: Regex = Regex::new(r"\d+/*$").expect("Bad message ID regex.");
    }

    let mat = MESSAGE_ID_REGEX.find(link).expect("MESSAGE_ID_REGEX couldn't match link.");

    let mut unsanitized_message_id = mat.as_str();

    if let Some(slash_pos) = unsanitized_message_id.find('/') {
        unsanitized_message_id = &unsanitized_message_id[0..slash_pos];
    }

    let message_id = unsanitized_message_id.parse().expect(
        "Message ID could not be parsed in link. \
    This should never happen.",
    );

    message_id
}

struct Log {
    pub user_id: u64,
    pub entry_id: usize,
    pub original_link: String,
    pub last_edited_link: Option<String>,
    pub reason: String,
}

impl Log {
    pub fn new(user_id: u64, entry_id: usize, original_link: String, last_edited_link: Option<String>, reason: String) -> Log {
        Log {
            user_id,
            entry_id,
            original_link,
            last_edited_link,
            reason,
        }
    }

    pub fn get_original_time(&self) -> i64 {
        let message_id = get_message_id_from_link(self.original_link.as_str());

        MessageId::from(message_id).created_at().timestamp()
    }

    pub fn get_edited_time(&self) -> Option<i64> {
        self.last_edited_link.as_ref().map(|last_edited_link| {
            let message_id = get_message_id_from_link(last_edited_link.as_str());

            MessageId::from(message_id).created_at().timestamp()
        })
    }
}

async fn parse_staff_log_member(ctx: &Context, msg: &Message, args: &mut Args, arg_pos: u32, args_needed: u32) -> Result<Member, CommandError> {
    let member = util::parse_member(ctx, msg, ArgumentInfo::new(args, arg_pos, args_needed)).await?;

    if member.user != msg.author {
        Ok(member)
    } else {
        args.rewind();

        let arg = args.current().expect("Argument that should exist doesn't.").to_string();

        args.advance();

        let reply = "You cannot read or modify your own staff log.";

        msg.channel_id.send_message(ctx, |msg| msg.content(reply)).await?;

        Err(Box::new(ArgumentParseErrorType::ArgumentConversionError::<u32>(
            ArgumentConversionError::new(arg),
        )))
    }
}

fn get_staff_logs(id: u64) -> Result<Vec<Log>, Error> {
    let connection = Connection::open(BURDBOT_DB)?;
    let query = "
        SELECT *
        FROM staff_logs
        WHERE user_id = ?;
    ";
    let mut statement = connection.prepare(query)?;
    let rows = statement
        .query_map([id], |row| {
            let original_link = row.get("original_link")?;
            let edited_link = row.get("last_edited_link")?;

            Ok(Log::new(id, row.get("entry_id")?, original_link, edited_link, row.get("reason")?))
        })?
        .map(|row| row.expect("Unwrapping this row should always be ok."))
        .collect();

    Ok(rows)
}

fn id_to_color(id: u64) -> Color {
    let id_bytes = id.to_le_bytes();
    let red = id_bytes[0] ^ id_bytes[7] ^ id_bytes[4];
    let green = id_bytes[1] ^ id_bytes[6] ^ id_bytes[3];
    let blue = id_bytes[2] ^ id_bytes[5];

    Color::from_rgb(red, green, blue)
}

fn format_field(log: &Log, is_first: bool) -> String {
    let edited_time = log.get_edited_time();
    let last_edited_text = match edited_time {
        Some(last_edited_time) => format!("**Last edited on**: <t:{}:f>\n", last_edited_time),
        None => String::new(),
    };

    let last_edited_link = match edited_time {
        Some(last_edited_time) => format!("\n[See last edit]({})", last_edited_time),
        None => String::new(),
    };

    if is_first {
        format!(
            "**Log #{}**:\n**Logged on**: <t:{}:f>\n{}**Reason**: {}\n[See original log]({}){}",
            log.entry_id,
            log.get_original_time(),
            last_edited_text,
            log.reason,
            log.original_link,
            last_edited_link
        )
    } else {
        format!(
            "**Logged on**: <t:{}:f>\n{}**Reason**: {}\n[See original log]({}){}",
            log.get_original_time(),
            last_edited_text,
            log.reason,
            log.original_link,
            last_edited_link
        )
    }
}

fn make_staff_log_embed(message: &mut CreateMessage, member: &Member) {
    let id = member.user.id.0;

    match get_staff_logs(id) {
        Ok(mut logs) => {
            logs.sort_by(|a, b| a.entry_id.cmp(&b.entry_id));

            message.embed(|embed| {
                let username = member.user.tag();
                let nickname = member.display_name();
                let avatar = member.avatar_url().unwrap_or(member.user.default_avatar_url());

                embed.title("Staff Log");
                embed.color(id_to_color(id));
                embed.author(|author| {
                    author.name(format!("{} ({})\n{}", username, nickname, id));
                    author.icon_url(avatar)
                });

                if logs.is_empty() {
                    embed.field("This user has no logs.", "", false)
                } else {
                    embed.field("⁣Log #1:", format_field(&logs[0], true), false);

                    for log in logs.iter().skip(1) {
                        embed.field("⁣", format_field(log, false), false);
                    }

                    embed
                }
            });
        }
        Err(error) => {
            error!("Error while making staff log embed: {:?}", error);

            message.content("Something's gone wrong. <@367538590520967181> has been notified.");
        }
    }
}

#[command]
#[description(
    "Displays the staff log of someone. Staff logs can only be seen by \
    administrators as long as it is not their own log."
)]
#[usage("<USER>")]
#[example("367538590520967181")]
#[example("DELIBURD#7741")]
#[aliases("slog", "sl")]
#[bucket("db_operations")]
async fn stafflog(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let target = parse_staff_log_member(ctx, msg, &mut args, 1, 1).await?;

    msg.channel_id
        .send_message(&ctx, |m| {
            make_staff_log_embed(m, &target);

            m
        })
        .await?;

    Ok(())
}

#[command]
#[description(
    "Adds a staff log entry. Staff logs can only be added by \
    administrators as long as it is not their own log."
)]
#[usage("<USER> <ENTRY>")]
#[example("367538590520967181 For being a bad burd")]
#[example("DELIBURD#7741 For being a bad burd")]
#[aliases("addslog", "addsl", "asl")]
async fn addstafflog(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let target_id = parse_staff_log_member(ctx, msg, &mut args, 1, 1).await?.user.id;

    Ok(())
}

#[command]
#[description(
    "Edits a staff log entry. Staff logs can only be edited by \
    administrators as long as it is not their own log."
)]
#[usage("<USER> <ENTRY NUMBER> <NEW ENTRY>")]
#[example("367538590520967181 1 Threw too many presents")]
#[example("DELIBURD#7741 1 Threw too many presents")]
#[aliases("editslog", "editsl", "esl")]
async fn editstafflog(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let target_id = parse_staff_log_member(ctx, msg, &mut args, 1, 1).await?.user.id;

    Ok(())
}

#[command]
#[description(
    "Removes a staff log entry. Staff logs can only be edited by \
    administrators as long as it is not their own log."
)]
#[usage("<USER> <ENTRY NUMBER>")]
#[example("367538590520967181 1")]
#[example("DELIBURD#7741 1")]
#[aliases("removeslog", "removesl", "rmsl")]
async fn removestafflog(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let target_id = parse_staff_log_member(ctx, msg, &mut args, 1, 1).await?.user.id;

    Ok(())
}

#[group]
#[only_in("guilds")]
#[commands(stafflog, addstafflog, editstafflog, removestafflog)]
#[required_permissions("Administrator")]
struct Administrative;
