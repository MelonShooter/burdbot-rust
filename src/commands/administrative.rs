use serenity::client::Context;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::{Args, CommandError, CommandResult};
use serenity::model::channel::Message;
use serenity::model::guild::Member;

use super::error_util::error::{ArgumentConversionError, ArgumentParseErrorType};
use super::{util, ArgumentInfo};

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
    let target_id = parse_staff_log_member(ctx, msg, &mut args, 1, 1).await?.user.id;

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
