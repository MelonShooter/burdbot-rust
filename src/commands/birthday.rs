use serenity::client::Context;
use std::collections::HashMap;
use std::time::Duration;
use std::u32;
use util::BoundedArgumentInfo;

use serenity::framework::standard::{Args, CommandResult};

use serenity::framework::standard::macros::{command, group};

use serenity::model::channel::Message;
use serenity::prelude::TypeMapKey;

use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use log::error;

use crate::birthday_tracker::add_birthday_to_db;

use super::{error_util, util};
use error_util::error::SerenitySQLiteError as Error;

const MONTH_TO_DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
const MONTH_TO_NAME: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub struct BirthdayInfoConfirmation {
    pub user_id: u64,
    pub month: u32,
    pub day: u32,
    pub time_zone: i64,
    pub is_privileged: bool,
    handle: JoinHandle<()>,
}

impl BirthdayInfoConfirmation {
    pub fn new(user_id: u64, month: u32, day: u32, time_zone: i64, handle: JoinHandle<()>, is_privileged: bool) -> BirthdayInfoConfirmation {
        BirthdayInfoConfirmation {
            user_id,
            month,
            day,
            time_zone,
            handle,
            is_privileged,
        }
    }
}

struct BirthdayInfoConfirmationKey;

impl TypeMapKey for BirthdayInfoConfirmationKey {
    type Value = RwLock<HashMap<u64, BirthdayInfoConfirmation>>;
}

#[command]
#[description(
    "Sets your birthday so that you get a special role for the day. Make sure the time zone you select is the \
        correct time zone for the given date. (Take into account daylight savings if needed.)"
)]
#[usage("<MONTH> <DAY> <UTC TIME ZONE ON DATE>")]
#[example(",,,setmybirthday 10 6 -7")]
#[example(",,,setmybday 10 6 7")]
#[aliases("setmybday")]
#[bucket("db_operations")]
async fn setmybirthday(context: &Context, message: &Message, mut args: Args) -> CommandResult {
    args.trimmed();

    set_birthday(context, message, args, message.author.id.0, false).await
}


// Make remove bday command which is privileged (see birthday_manager.rs)
// Finish get birthday command (for self and also for others, see birthday_manager.rs)
// make command to add, get, and remove bday role (see birthday_server_role_manager.rs)
// to run the privileged birthday commands, must have manage roles permission

#[command]
#[required_permissions(MANAGE_ROLES)]
#[description(
    "Sets a user's birthday so that they get a special role for the day. Make sure the time zone selected is the \
        correct time zone for the given date. (Take into account daylight savings if needed.)"
)]
#[usage("<USER> <MONTH> <DAY> <UTC TIME ZONE ON DATE>")]
#[example(",,,setuserbirthday 367538590520967181 10 6 -7")]
#[example(",,,setusrbday DELIBURD#7741 10 6 7")]
#[aliases("setuserbirthday", "setusrbday", "setusrbirthday", "setuserbday")]
#[bucket("db_operations")]
async fn setuserbirthday(context: &Context, message: &Message, mut args: Args) -> CommandResult {
    args.trimmed();

    let user_id = util::parse_user(context, message, &mut args).await?;

    set_birthday(context, message, args, user_id, true).await
}

async fn set_birthday(context: &Context, message: &Message, mut args: Args, target_id: u64, is_privileged: bool) -> CommandResult {
    args.quoted();

    let month_arg_info = BoundedArgumentInfo::new(&mut args, 1, 3, 1, 12);
    let month = util::parse_bounded_arg(context, message, month_arg_info).await? as u32;
    let month_index = (month - 1) as usize;

    let max_day_count = MONTH_TO_DAYS[month_index];
    let day_arg_info = BoundedArgumentInfo::new(&mut args, 2, 3, 1, max_day_count);
    let day = util::parse_bounded_arg(context, message, day_arg_info).await? as u32;

    let time_zone_arg_info = BoundedArgumentInfo::new(&mut args, 3, 3, -11, 14);
    let time_zone = util::parse_bounded_arg(context, message, time_zone_arg_info).await?;

    {
        let mut data = context.data.write().await;

        if !data.contains_key::<BirthdayInfoConfirmationKey>() {
            data.insert::<BirthdayInfoConfirmationKey>(RwLock::new(HashMap::new()));
        }
    }

    let mut time_zone_string: String;

    if time_zone >= 0 {
        time_zone_string = String::with_capacity(3);
        time_zone_string.push('+');
        time_zone_string.push_str(time_zone.to_string().as_str());
    } else {
        time_zone_string = time_zone.to_string();
    }
;
    let birthday_set_message;

    if !is_privileged {
        birthday_set_message = format!(
            "Your birthday will be set as ``{} {}``. You will get the birthday role for 24 \
                hours starting at 0:00 UTC{} of that day. Are you sure this is what you want? You won't be able to change this again \
                unless a moderator does it for you. Type ``,,,birthdayconfirm`` to confirm this. This will expire in 30 seconds.",
            MONTH_TO_NAME[month_index], day, time_zone_string
        );
    } else {
        birthday_set_message = format!(
            "{}'s birthday will be set as ``{} {}``. They will get the birthday role for 24 \
                hours starting at 0:00 UTC{} of that day. Are you sure this is what you want? \
                Type ``,,,birthdayconfirm`` to confirm this. This will expire in 30 seconds.",
            target_id, MONTH_TO_NAME[month_index], day, time_zone_string
        );
    }

    let channel_id = message.channel_id;


    util::send_message(context, &channel_id, birthday_set_message, "setbirthday").await;

    let ctx_data = context.data.clone();
    let ctx_http = context.http.clone();
    let author_id = *message.author.id.as_u64();
    let handle = tokio::spawn(async move {
        sleep(Duration::from_millis(30000)).await;

        let data = ctx_data.read().await;
        let mut birthday_info_map = data.get::<BirthdayInfoConfirmationKey>().unwrap().write().await;

        util::send_message(&ctx_http, &channel_id, "Add birthday request expired.", "setbirthday").await;

        birthday_info_map.remove(&author_id);
    });

    let data = context.data.read().await;
    let mut birthday_info_map = data.get::<BirthdayInfoConfirmationKey>().unwrap().write().await;
    let info = BirthdayInfoConfirmation::new(target_id, month, day, time_zone, handle, is_privileged);

    if let Some(old_info) = birthday_info_map.insert(author_id, info) {
        old_info.handle.abort(); // Abort the old timed remove.
    }

    Ok(())
}

#[command]
#[description("Confirms a birthday set with a previous command.")]
#[aliases("bdayconfirm")]
#[bucket("default")]
async fn birthdayconfirm(context: &Context, message: &Message) -> CommandResult {
    let data = context.data.read().await;
    let birthday_info_map = data.get::<BirthdayInfoConfirmationKey>().unwrap().read().await;

    if let Some(info) = birthday_info_map.get(message.author.id.as_u64()) {
        if let Err(error) = add_birthday_to_db(context, &message.channel_id, info).await {
            match error {
                Error::SerenityError(errors) => error!("Serenity error while adding birthday to db: {}", errors[0]),
                Error::SQLiteError(error) => error!("SQLite error while adding birthday to db: {}", error),
            }

            error_util::generic_fail(context, &message.channel_id).await;
        }
    } else {
        let set_first_message = "Set your birthday first with ,,,setmybirthday if you're setting your own birthday \
            or with ,,,setuserbirthday if you're setting someone else's birthday.";

        util::send_message(context, &message.channel_id, set_first_message, "birthdayconfirm").await;
    }

    Ok(())
}

#[command]
#[help_available(false)]
#[bucket("default")]
async fn chamuyar(context: &Context, message: &Message, mut args: Args) -> CommandResult {
    let msg_to_send: String;

    if let Ok(person) = args.single::<String>() {
        if person != "@everyone" && person != "@here" {
            msg_to_send = format!("Alta facha tiene el {}", person);
        } else {
            util::send_message(context, &message.channel_id, "Nice try.", "chamuyar").await;

            return Ok(());
        }
    } else {
        error_util::not_enough_arguments(context, &message.channel_id, 0, 1).await;

        return Ok(());
    }

    util::send_message(context, &message.channel_id, msg_to_send, "chamuyar").await;

    Ok(())
}

#[group]
#[commands(setmybirthday, birthdayconfirm, chamuyar, setuserbirthday)]
#[only_in("guilds")]
struct Birthday;
