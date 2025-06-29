use serenity::client::Context;
use serenity::model::Permissions;

use std::collections::HashMap;
use std::time::Duration;

use serenity::framework::standard::{Args, CommandResult};

use serenity::framework::standard::macros::{command, group};

use serenity::model::channel::Message;
use serenity::prelude::TypeMapKey;

use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use log::error;

use crate::argument_parser::{self, ArgumentInfo, BoundedArgumentInfo};
use crate::birthday_tracker::{self, add_birthday_to_db};
use crate::commands::error_util;
use crate::error::SerenitySQLiteError;
use crate::util;

pub const MONTH_TO_DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
pub const MONTH_TO_NAME: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September",
    "October", "November", "December",
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
    pub fn new(
        user_id: u64, month: u32, day: u32, time_zone: i64, handle: JoinHandle<()>,
        is_privileged: bool,
    ) -> BirthdayInfoConfirmation {
        BirthdayInfoConfirmation { user_id, month, day, time_zone, is_privileged, handle }
    }
}

struct BirthdayInfoConfirmationKey;

impl TypeMapKey for BirthdayInfoConfirmationKey {
    type Value = RwLock<HashMap<u64, BirthdayInfoConfirmation>>;
}

#[command]
#[only_in("guilds")]
#[description(
    "Sets your birthday so that you get a special role for the day. Make sure the time zone you select is the \
        correct time zone for the given date. (Take into account daylight savings if needed.)"
)]
#[usage("<MONTH> <DAY> <UTC TIME ZONE ON DATE>")]
#[example("10 6 -7")]
#[example("10 6 7")]
#[aliases("setmybday")]
#[bucket("db_operations")]
async fn setmybirthday(context: &Context, message: &Message, mut args: Args) -> CommandResult {
    args.trimmed();

    let cache = &context.cache;
    let guild_id = message.guild_id.unwrap();
    let user_id = message.author.id;
    let permissions = util::get_member_permissions(cache, guild_id, user_id).await;
    let is_privileged_option = permissions.map(Permissions::manage_roles);

    if let Some(is_privileged) = is_privileged_option {
        set_birthday(context, message, args, message.author.id.get(), is_privileged).await
    } else {
        Ok(())
    }
}

#[command]
#[only_in("guilds")]
#[required_permissions(MANAGE_ROLES)]
#[description(
    "Sets a user's birthday so that they get a special role for the day. Make sure the time zone selected is the \
        correct time zone for the given date. (Take into account daylight savings if needed.)"
)]
#[usage("<USER> <MONTH> <DAY> <UTC TIME ZONE ON DATE>")]
#[example("367538590520967181 10 6 -7")]
#[example("DELIBURD#7741 10 6 7")]
#[aliases("setusrbday", "setuserbday")]
#[bucket("db_operations")]
async fn setuserbirthday(context: &Context, message: &Message, mut args: Args) -> CommandResult {
    args.trimmed();

    let arg_info = ArgumentInfo::new(&mut args, 1, 4);
    let member = argument_parser::parse_member(context, message, arg_info).await?;

    set_birthday(context, message, args, member.user.id.get(), true).await
}

async fn set_birthday(
    context: &Context, message: &Message, mut args: Args, target_id: u64, is_privileged: bool,
) -> CommandResult {
    args.quoted();

    let month_arg_info = BoundedArgumentInfo::new(&mut args, 1, 3, 1, 12);
    let month = argument_parser::parse_bounded_arg(context, message, month_arg_info).await? as u32;
    let month_index = (month - 1) as usize;

    let max_day_count = MONTH_TO_DAYS[month_index];
    let day_arg_info = BoundedArgumentInfo::new(&mut args, 2, 3, 1, max_day_count);
    let day = argument_parser::parse_bounded_arg(context, message, day_arg_info).await? as u32;

    let time_zone_arg_info = BoundedArgumentInfo::new(&mut args, 3, 3, -11, 14);
    let time_zone =
        argument_parser::parse_bounded_arg(context, message, time_zone_arg_info).await?;

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
    };

    let birthday_set_message = if is_privileged {
        format!(
            "{}'s birthday will be set as ``{} {}``. They will get the birthday role for 24 \
                hours starting at 0:00 UTC{} of that day. Are you sure this is what you want? \
                Type ``{}birthdayconfirm`` to confirm this. This will expire in 30 seconds.",
            target_id,
            MONTH_TO_NAME[month_index],
            day,
            time_zone_string,
            crate::PREFIX
        )
    } else {
        format!(
            "Your birthday will be set as ``{} {}``. You will get the birthday role for 24 \
                hours starting at 0:00 UTC{} of that day. Are you sure this is what you want? You won't be able to change this again \
                unless a moderator does it for you. Type ``{}birthdayconfirm`` to confirm this. This will expire in 30 seconds.",
            MONTH_TO_NAME[month_index],
            day,
            time_zone_string,
            crate::PREFIX
        )
    };

    let channel_id = message.channel_id;

    util::send_message(context, channel_id, birthday_set_message, "setbirthday").await;

    let ctx_data = context.data.clone();
    let ctx_http = context.http.clone();
    let author_id = message.author.id.get();
    let handle = tokio::spawn(async move {
        sleep(Duration::from_millis(30000)).await;

        let data = ctx_data.read().await;
        let mut birthday_info_map =
            data.get::<BirthdayInfoConfirmationKey>().unwrap().write().await;

        util::send_message(&ctx_http, channel_id, "Add birthday request expired.", "setbirthday")
            .await;

        birthday_info_map.remove(&author_id);
    });

    let data = context.data.read().await;
    let mut birthday_info_map = data.get::<BirthdayInfoConfirmationKey>().unwrap().write().await;
    let info =
        BirthdayInfoConfirmation::new(target_id, month, day, time_zone, handle, is_privileged);

    if let Some(old_info) = birthday_info_map.insert(author_id, info) {
        old_info.handle.abort(); // Abort the old timed remove.
    }

    Ok(())
}

#[command]
#[only_in("guilds")]
#[description("Confirms a birthday set with a previous command.")]
#[aliases("bdayconfirm")]
#[bucket("default")]
async fn birthdayconfirm(context: &Context, message: &Message) -> CommandResult {
    let data = context.data.read().await;
    let birthday_info_map_lock_option = data.get::<BirthdayInfoConfirmationKey>();
    let birthday_info_map;

    if let Some(birthday_info_map_lock) = birthday_info_map_lock_option {
        birthday_info_map = birthday_info_map_lock.read().await;

        if let Some(info) = birthday_info_map.get(&message.author.id.get()) {
            info.handle.abort(); // Abort the request expired message

            if let Err(error) =
                add_birthday_to_db(context, message.guild_id.unwrap(), message.channel_id, info)
                    .await
            {
                match error {
                    SerenitySQLiteError::SerenityError(errors) => error!(
                        "Serenity error while adding birthday to db: {}",
                        errors.serenity_errors[0]
                    ),
                    SerenitySQLiteError::SQLiteError(error) => {
                        error!("SQLite error while adding birthday to db: {error}")
                    },
                }

                error_util::generic_fail(context, message.channel_id).await;
            }

            return Ok(());
        }
    }

    let set_first_message = format!(
        "Set your birthday first with {}setmybirthday if you're setting your own birthday \
    or with {}setuserbirthday if you're setting someone else's birthday.",
        crate::PREFIX,
        crate::PREFIX
    );

    util::send_message(context, message.channel_id, set_first_message, "birthdayconfirm").await;

    Ok(())
}

#[command]
#[only_in("guilds")]
#[required_permissions(MANAGE_ROLES)]
#[description(
    "Removes a user's birthday so that they don't get any special roles on the configured day."
)]
#[usage("<USER>")]
#[example("367538590520967181")]
#[example("DELIBURD#7741")]
#[aliases("removeusrbday", "removeuserbday", "rmusrbday", "rmuserbday")]
#[bucket("db_operations")]
async fn removeuserbirthday(context: &Context, message: &Message, mut args: Args) -> CommandResult {
    let arg_info = ArgumentInfo::new(&mut args, 1, 1);
    let user_id = argument_parser::parse_member(context, message, arg_info).await?.user.id;
    let guild_id = message.guild_id.unwrap();

    birthday_tracker::remove_birthday(context, message.channel_id, guild_id, user_id).await?;

    Ok(())
}

#[command]
#[only_in("guilds")]
#[description("Gets your birthday.")]
#[aliases("getmybday")]
#[bucket("db_operations")]
async fn getmybirthday(context: &Context, message: &Message) -> CommandResult {
    let user_id = message.author.id.get();

    birthday_tracker::get_birthday(context, message.channel_id, user_id).await?;

    Ok(())
}

#[command]
#[only_in("guilds")]
#[required_permissions(MANAGE_ROLES)]
#[description("Gets a user's birthday.")]
#[usage("<USER>")]
#[example("367538590520967181")]
#[example("DELIBURD#7741")]
#[aliases("getusrbday", "getusrbirthday", "getuserbday")]
#[bucket("db_operations")]
async fn getuserbirthday(context: &Context, message: &Message, mut args: Args) -> CommandResult {
    let arg_info = ArgumentInfo::new(&mut args, 1, 1);
    let member = argument_parser::parse_member(context, message, arg_info).await?;

    birthday_tracker::get_birthday(context, message.channel_id, member.user.id.get()).await?;

    Ok(())
}

#[command]
#[only_in("guilds")]
#[required_permissions(MANAGE_ROLES)]
#[description("Set a role to give to users when it's their birthday.")]
#[usage("<ROLE>")]
#[example("728359316498808895")]
#[example("@Birthday Role")]
#[aliases("setserverbdayrole", "setsvbdayrole")]
#[bucket("very_intense")]
async fn setserverbirthdayrole(
    context: &Context, message: &Message, mut args: Args,
) -> CommandResult {
    let arg_info = ArgumentInfo::new(&mut args, 1, 1);
    let role_id = argument_parser::parse_role(context, message, arg_info).await?.get();
    let guild_id = message.guild_id.unwrap().get();

    birthday_tracker::set_birthday_role(context, message.channel_id, guild_id, role_id).await?;

    Ok(())
}

#[command]
#[only_in("guilds")]
#[required_permissions(MANAGE_ROLES)]
#[description("Gets the role to give to users when it's their birthday.")]
#[aliases("getserverbdayrole", "getsvbdayrole")]
#[bucket("db_operations")]
async fn getserverbirthdayrole(context: &Context, message: &Message) -> CommandResult {
    let guild_id = message.guild_id.unwrap().get();

    birthday_tracker::get_birthday_role(context, message.channel_id, guild_id).await?;

    Ok(())
}

#[command]
#[only_in("guilds")]
#[required_permissions(MANAGE_ROLES)]
#[description("Removes the role to give to users when it's their birthday.")]
#[aliases("removeserverbdayrole", "rmserverbdayrole", "removesvbdayrole", "rmsvbdayrole")]
#[bucket("db_operations")]
async fn removeserverbirthdayrole(context: &Context, message: &Message) -> CommandResult {
    birthday_tracker::remove_birthday_role(context, message.channel_id, message.guild_id.unwrap())
        .await?;

    Ok(())
}

#[group]
#[commands(
    setmybirthday, birthdayconfirm, setuserbirthday, removeuserbirthday, getuserbirthday,
    getmybirthday, setserverbirthdayrole, getserverbirthdayrole, removeserverbirthdayrole
)]
struct Birthday;
