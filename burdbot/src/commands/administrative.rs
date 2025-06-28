use lazy_static::lazy_static;
use log::error;
use regex::Regex;
use rusqlite::{Connection, params};
use serenity::all::{CreateEmbedAuthor, CreateEmbedFooter};
use serenity::builder::{CreateEmbed, CreateMessage};
use serenity::client::Context;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{Args, CommandResult};
use serenity::model::Color;
use serenity::model::channel::Message;
use serenity::model::guild::Member;
use serenity::model::id::MessageId;
use serenity::model::prelude::User;

use crate::{BURDBOT_DB, argument_parser};

use crate::argument_parser::{
    ArgumentConversionError, ArgumentInfo, ArgumentParseError, BoundedArgumentInfo, ConversionType,
};

const GONE_WRONG: &str = "Something's gone wrong. <@367538590520967181> has been notified.";

fn get_message_id_from_link(link: &str) -> u64 {
    lazy_static! {
        static ref MESSAGE_ID_REGEX: Regex = Regex::new(r"\d+/*$").expect("Bad message ID regex.");
    }

    let mat = MESSAGE_ID_REGEX.find(link).expect("MESSAGE_ID_REGEX couldn't match link.");

    let mut unsanitized_message_id = mat.as_str();

    if let Some(slash_pos) = unsanitized_message_id.find('/') {
        unsanitized_message_id = &unsanitized_message_id[0..slash_pos];
    }

    unsanitized_message_id.parse().expect(
        "Message ID could not be parsed in link. \
    This should never happen.",
    )
}

struct Log {
    entry_id: i64,
    original_link: String,
    last_edited_link: Option<String>,
    reason: String,
}

impl Log {
    fn new(
        entry_id: i64, original_link: String, last_edited_link: Option<String>, reason: String,
    ) -> Log {
        Log { entry_id, original_link, last_edited_link, reason }
    }

    fn get_original_time(&self) -> i64 {
        let message_id = get_message_id_from_link(self.original_link.as_str());

        MessageId::from(message_id).created_at().timestamp()
    }

    fn get_edited_time(&self) -> Option<i64> {
        self.last_edited_link.as_ref().map(|last_edited_link| {
            let message_id = get_message_id_from_link(last_edited_link.as_str());

            MessageId::from(message_id).created_at().timestamp()
        })
    }
}

async fn parse_staff_log_member(
    ctx: &Context, msg: &Message, args: &mut Args, arg_pos: usize, args_needed: usize,
) -> CommandResult<Member> {
    let member =
        argument_parser::parse_member(ctx, msg, ArgumentInfo::new(args, arg_pos, args_needed))
            .await?;

    if member.user == msg.author {
        args.rewind();

        let arg = args.current().expect("Argument that should exist doesn't.").to_string();

        args.advance();

        let reply = "You cannot read or modify your own staff log.";

        msg.channel_id.send_message(ctx, CreateMessage::new().content(reply)).await?;

        Err(Box::new(ArgumentParseError::ArgumentConversionError(ArgumentConversionError::new(
            arg_pos,
            arg,
            ConversionType::NonSelfMember,
        ))))
    } else {
        Ok(member)
    }
}

fn get_staff_logs(id: u64) -> rusqlite::Result<Vec<Log>> {
    let connection = Connection::open(BURDBOT_DB)?;
    let query = "
        SELECT original_link, last_edited_link, reason
        FROM staff_logs
        WHERE user_id = ?
        ORDER BY entry_id;
    ";
    let mut statement = connection.prepare(query)?;
    let rows = statement
        .query_map([id], |row| {
            let original_link = row.get("original_link")?;
            let edited_link = row.get("last_edited_link")?;

            Ok(Log::new(0, original_link, edited_link, row.get("reason")?))
        })?
        .enumerate()
        .map(|(index, row_result)| {
            let mut row = row_result.expect("Unwrapping this row should always be ok.");

            row.entry_id = index as i64 + 1;

            row
        })
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
        Some(last_edited_time) => format!("**Last edited on**: <t:{last_edited_time}:f>\n"),
        None => String::new(),
    };

    let last_edited_link = match &log.last_edited_link {
        Some(edit_link) => format!("\n[See last edit]({edit_link})"),
        None => String::new(),
    };

    if is_first {
        format!(
            "**Logged on**: <t:{}:f>\n{}**Reason**: {}\n[See original log]({}){}",
            log.get_original_time(),
            last_edited_text,
            log.reason,
            log.original_link,
            last_edited_link
        )
    } else {
        format!(
            "**Log #{}**:\n**Logged on**: <t:{}:f>\n{}**Reason**: {}\n[See original log]({}){}",
            log.entry_id,
            log.get_original_time(),
            last_edited_text,
            log.reason,
            log.original_link,
            last_edited_link
        )
    }
}

fn make_staff_log_embed<F>(invoker: &User, member: &Member, func: F) -> (Option<i64>, CreateMessage)
where
    F: FnOnce(CreateEmbed, i64) -> CreateEmbed,
{
    let id = member.user.id.get();
    let mut message = CreateMessage::new();

    match get_staff_logs(id) {
        Ok(logs) => {
            let log_count = logs.len() as i64;
            let username = member.user.tag();
            let nickname = member.display_name();
            let avatar =
                member.user.avatar_url().unwrap_or_else(|| member.user.default_avatar_url());
            let author =
                CreateEmbedAuthor::new(format!("{username} ({nickname})\n{id}")).icon_url(avatar);
            let embed_footer = CreateEmbedFooter::new(format!("Requested by: {}", invoker.tag()))
                .icon_url(invoker.avatar_url().unwrap_or_else(|| invoker.default_avatar_url()));
            let mut embed = CreateEmbed::new()
                .title("Staff Log")
                .color(id_to_color(id))
                .author(author)
                .footer(embed_footer);

            if logs.is_empty() {
                embed = embed.description("This user has no logs.");
            } else {
                embed = embed.field("⁣Log #1:", format_field(&logs[0], true), false);

                for log in logs.iter().skip(1) {
                    embed = embed.field("⁣", format_field(log, false), false);
                }
            }

            embed = func(embed, log_count);
            (Some(log_count), message.embed(embed))
        },
        Err(error) => {
            error!("Error while making staff log embed: {error:?}");

            message =
                message.content("Something's gone wrong. <@367538590520967181> has been notified.");

            (None, message)
        },
    }
}

fn add_log(user_id: u64, entry_id: i64, original_link: &str, reason: &str) -> rusqlite::Result<()> {
    let connection = Connection::open(BURDBOT_DB)?;
    let insert_query = "
            INSERT INTO staff_logs
                VALUES(?, ?, ?, ?, ?);
        ";

    connection
        .execute(insert_query, params![user_id, entry_id, original_link, None::<u8>, reason])?;

    Ok(())
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
        .send_message(&ctx, make_staff_log_embed(&msg.author, &target, |e, _| e).1)
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
    let target = parse_staff_log_member(ctx, msg, &mut args, 1, 2).await?;
    let target_id = target.user.id.get();
    let reason = match args.remains() {
        Some(reason) => reason,
        None => {
            msg.channel_id.say(ctx, "You must specify a reason for the log.").await?;

            return Ok(());
        },
    };

    let msg_link = msg.link();
    let (log_ct, mut msg_content) =
        make_staff_log_embed(&msg.author, &target, |embed, log_count| {
            let log = &Log::new(log_count + 1, msg_link.clone(), None, reason.to_string());

            if log_count == 0 {
                embed.field("⁣Log #1:", format_field(log, true), false)
            } else {
                embed.field("⁣", format_field(log, false), false)
            }
        });

    if let Some(ct) = log_ct {
        let entry_id = ct + 1;

        if let Err(err) = add_log(target_id, entry_id, msg_link.as_str(), reason) {
            error!("Error while making staff log embed: {err:?}");

            msg_content = msg_content.content(GONE_WRONG)
        }
    }

    msg.channel_id.send_message(ctx, msg_content).await?;

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
    let target = parse_staff_log_member(ctx, msg, &mut args, 1, 3).await?;
    let entry_id = argument_parser::parse_bounded_arg(
        ctx,
        msg,
        BoundedArgumentInfo::new(&mut args, 1, 3, 1, i64::MAX),
    )
    .await?;
    let target_id = target.user.id.get();
    let reason = match args.remains() {
        Some(reason) => reason,
        None => {
            msg.channel_id.say(ctx, "You must specify a reason for the log.").await?;

            return Ok(());
        },
    };

    let rows_changed;

    {
        let connection = Connection::open(BURDBOT_DB)?;
        let update_query = "
            UPDATE staff_logs
                SET(last_edited_link, reason) = (?, ?)
                WHERE user_id = ? AND entry_id = ?;
        ";

        rows_changed =
            connection.execute(update_query, params![msg.link(), reason, target_id, entry_id])?;
    }

    let msg_content = if rows_changed > 0 {
        let (log_ct, mut m) = make_staff_log_embed(&msg.author, &target, |c, _| c);

        if log_ct.is_some() {
            // If successful, then set msg content
            m = m.content("Edited staff log.")
        }

        m
    } else {
        CreateMessage::new().content(
            "Could not find the given log entry. Please verify that this log entry exists.",
        )
    };

    msg.channel_id.send_message(ctx, msg_content).await?;

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
    let target = parse_staff_log_member(ctx, msg, &mut args, 1, 2).await?;
    let entry_id = argument_parser::parse_bounded_arg(
        ctx,
        msg,
        BoundedArgumentInfo::new(&mut args, 2, 2, 1, i64::MAX),
    )
    .await?;
    let target_id = target.user.id.get();

    let rows_changed;

    {
        let mut connection = Connection::open(BURDBOT_DB)?;
        let transaction = connection.transaction()?;
        let delete_query = "
            DELETE FROM staff_logs
            WHERE user_id = ? AND entry_id = ?;
        ";

        rows_changed = transaction.execute(delete_query, params![target_id, entry_id])?;

        // Update the other entries after this entry id to decrement their ids.
        if rows_changed != 0 {
            let decrement_entry_ids = "
                UPDATE staff_logs
                    SET entry_id = entry_id - 1
                    WHERE user_id = ? AND entry_id > ?;
            ";

            transaction.execute(decrement_entry_ids, params![target_id, entry_id])?;
        }

        transaction.commit()?;
    }

    let msg_content = if rows_changed > 0 {
        let (log_ct, mut m) = make_staff_log_embed(&msg.author, &target, |c, _| c);

        if log_ct.is_some() {
            // If successful, then set msg content
            m = m.content("Successfully removed entry from staff log.")
        }

        m
    } else {
        CreateMessage::new().content(
            "Could not find the given log entry. Please verify that this log entry exists.",
        )
    };

    msg.channel_id.send_message(ctx, msg_content).await?;

    Ok(())
}

#[group]
#[only_in("guilds")]
#[commands(stafflog, addstafflog, editstafflog, removestafflog)]
#[required_permissions("Administrator")]
struct Administrative;
