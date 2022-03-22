use chrono::DateTime;
use chrono::Datelike;
use chrono::Duration;
use chrono::NaiveDate;
use chrono::Timelike;
use chrono::Utc;
use log::warn;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::Error;
use rusqlite::OptionalExtension;
use rusqlite::Transaction;
use serenity::client::Context;
use serenity::model::channel::GuildChannel;
use serenity::model::id::ChannelId;

use crate::commands;
use crate::commands::error_util::error::SerenitySQLiteError;
use crate::commands::BirthdayInfoConfirmation;
use crate::BURDBOT_DB;

use super::BirthdayDateTime;

fn get_server_role(transaction: &Transaction, guild_id: u64) -> Result<Option<u64>, Error> {
    let role_select_statement = "
    SELECT role_id FROM bday_role_list
    WHERE guild_id = ?";

    transaction
        .query_row(role_select_statement, [guild_id], |row| row.get::<_, u64>(0))
        .optional()
}

pub async fn add_birthday_to_db(ctx: &Context, channel_id: ChannelId, bday_info: &BirthdayInfoConfirmation) -> Result<(), SerenitySQLiteError> {
    let connection = Connection::open(BURDBOT_DB)?;
    let ins_stmt_str = if bday_info.is_privileged {
        "
        INSERT OR REPLACE INTO bday
        VALUES (?, ?, ?);
        "
    } else {
        "
            INSERT OR IGNORE INTO bday
            VALUES (?, ?, ?);
        "
    };

    let user_id = bday_info.user_id;
    let channel_selector = |channel: &GuildChannel| *channel.guild_id.as_u64();
    let guild_id = ctx.cache.guild_channel_field(channel_id, channel_selector).await.unwrap();
    let bday_date_naive_local = NaiveDate::from_ymd(2021, bday_info.month, bday_info.day).and_hms(0, 0, 0);
    let bday_date_naive_utc = bday_date_naive_local - Duration::hours(bday_info.time_zone);
    let bday_date_time = BirthdayDateTime::new(bday_date_naive_utc.month(), bday_date_naive_utc.day(), bday_date_naive_utc.hour());
    let rows_changed = connection.execute(ins_stmt_str, params!(user_id, guild_id, bday_date_time))?;

    if rows_changed == 0 {
        // Must be an unprivileged person trying to override their own birthday.
        let unprivileged_str = "You already set your birthday. Please ask a mod to override it if you need to.";

        commands::send_message(ctx, channel_id, unprivileged_str, "add_birthday_to_db").await;

        return Ok(());
    }

    let message;
    let role_id_option;

    {
        let mut role_check_query = connection.prepare(
            "
                SELECT role_id 
                FROM bday_role_list 
                WHERE guild_id = ?;
            ",
        )?;

        match role_check_query.query_row([guild_id], |row| Ok(row.get::<_, u64>(0))).optional()? {
            Some(role_id_result) => {
                role_id_option = Some(role_id_result?);
                message = format!("{}'s birthday has been saved.", user_id);
            }
            None => {
                role_id_option = None;
                message = format!(
                    "{}'s birthday has been saved, but this server doesn't have a birthday role. \
                    Please ask a staff member to set one.",
                    user_id
                );
            }
        };
    }

    if let Some(role_id) = role_id_option {
        let now = Utc::now().naive_utc();
        let bday_over = bday_date_naive_utc + Duration::days(1);

        // Check if the birthday is ongoing
        if now < bday_over && now > bday_date_naive_utc {
            let bday_date_time = BirthdayDateTime::new(bday_over.month(), bday_over.day(), bday_over.hour());
            let insertion_statement = "
                INSERT OR IGNORE INTO bday_user_list
                    VALUES (?, ?);
            ";

            connection.execute(insertion_statement, params!(user_id, bday_date_time))?;

            if let Err(error) = ctx.http.add_member_role(guild_id, user_id, role_id).await {
                warn!(
                    "Error while trying to add role to user while adding bday to db. Likely not a concern \
                        considering this most likely occurred because the role was removed while \
                        the code was executing or insufficient permission: {:?}",
                    error
                );
            }
        }
    }

    commands::send_message(ctx, channel_id, message, "add_birthday_to_db").await;

    Ok(())
}

pub async fn get_birthday(ctx: &Context, channel_id: ChannelId, user_id: u64) -> Result<(), SerenitySQLiteError> {
    let connection = Connection::open(BURDBOT_DB)?;
    let bday_select_str = "
            SELECT bday_date
            FROM bday
            WHERE user_id = ?";
    let bday_option = connection
        .query_row(bday_select_str, [user_id], |row| row.get::<_, BirthdayDateTime>(0))
        .optional()?;

    if let Some(bday) = bday_option {
        channel_id
            .send_message(&ctx.http, |msg| {
                msg.embed(|embed| {
                    let now = Utc::now();
                    let naive_timestamp = NaiveDate::from_ymd(now.year(), bday.month, bday.day).and_hms(bday.hour, 0, 0);
                    let mut time_stamp = DateTime::<Utc>::from_utc(naive_timestamp, Utc);

                    if time_stamp < now {
                        time_stamp = time_stamp.with_year(time_stamp.year() + 1).unwrap();
                    }

                    embed.timestamp(&time_stamp);

                    let msg = format!("{}'s next birthday will start at ", user_id);

                    embed.footer(|footer| footer.text(msg))
                })
            })
            .await?;
    } else {
        let msg = format!("No birthday found from the user {}", user_id);

        channel_id
            .send_message(&ctx.http, |message| message.embed(|embed| embed.description(msg)))
            .await?;
    }

    Ok(())
}

pub async fn remove_birthday(ctx: &Context, channel_id: ChannelId, guild_id: u64, user_id: u64) -> Result<(), SerenitySQLiteError> {
    let mut role_id = None;
    let rows_changed;

    {
        let mut connection = Connection::open(BURDBOT_DB)?;
        let transaction = connection.transaction()?;

        // Foreign key constraint will take care of people in the ongoing birthday table.
        transaction.execute(
            "
            DELETE FROM bday
            WHERE user_id = ?
        ",
            [user_id],
        )?;

        rows_changed = transaction
            .query_row("SELECT total_changes();", [], |row| row.get::<_, usize>(0))
            .unwrap();

        // If more than 1 row changed then we deleted a foreign key too from bday_user_list
        if rows_changed > 1 {
            role_id = get_server_role(&transaction, guild_id)?;
        }

        transaction.commit()?;
    }

    if let Some(id) = role_id {
        if let Err(error) = ctx.http.remove_member_role(guild_id, user_id, id).await {
            warn!(
                "Error while trying to remove birthday from database. Likely not a concern \
                    considering this most likely occurred because the role was removed while \
                    the code was executing or insufficient permission: {:?}",
                error
            );
        }
    }
    // Give this message only if their bday was actually found
    let message = if rows_changed > 0 {
        format!("{}'s birthday was removed.", user_id)
    } else {
        format!("No birthday was found for {}.", user_id)
    };

    commands::send_message(ctx, channel_id, message, "remove_birthday").await;

    Ok(())
}
