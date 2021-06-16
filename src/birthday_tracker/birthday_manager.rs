use chrono::Datelike;
use chrono::Duration;
use chrono::NaiveDate;
use chrono::Timelike;
use rusqlite::params;
use rusqlite::Connection;
use serenity::client::Context;
use serenity::model::id::ChannelId;

use crate::commands;
use crate::commands::error_util::error::SerenitySQLiteError;
use crate::commands::BirthdayInfoConfirmation;
use crate::BURDBOT_DB;

use super::BirthdayDateTime;

pub async fn add_birthday_to_db(ctx: &Context, channel_id: &ChannelId, bday_info: &BirthdayInfoConfirmation) -> Result<(), SerenitySQLiteError> {
    let connection = Connection::open(BURDBOT_DB)?;
    let ins_stmt_str;

    if !bday_info.is_privileged {
        ins_stmt_str = "
            INSERT OR IGNORE INTO bday
            VALUES (?, ?, ?);
        ";
    } else {
        ins_stmt_str = "
            INSERT OR REPLACE INTO bday
            VALUES (?, ?, ?);
        ";
    }

    let user_id = bday_info.user_id;
    let guild_id = *channel_id.to_channel(ctx).await?.guild().unwrap().guild_id.as_u64();
    let bday_date_naive_local = NaiveDate::from_ymd(2021, bday_info.month, bday_info.day).and_hms(0, 0, 0);
    let bday_date_naive_utc = bday_date_naive_local + Duration::hours(bday_info.time_zone);
    let bday_date_time = BirthdayDateTime::new(bday_date_naive_utc.month(), bday_date_naive_utc.day(), bday_date_naive_utc.hour());

    let rows_changed = connection.execute(ins_stmt_str, params!(user_id, guild_id, bday_date_time))?;

    if rows_changed == 0 {
        // Must be an unprivileged person trying to override their own birthday.
        let unprivileged_str = "You already set your birthday. Please ask a mod to override it if you need to.";
        commands::send_message(ctx, channel_id, unprivileged_str, "add_birthday_to_db").await;
    }

    Ok(())
}

pub fn get_birthday() {
    todo!()
}

pub fn remove_birthday() {
    todo!()
}
