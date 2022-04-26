use chrono::{DateTime, Datelike, Duration, Utc};
use rusqlite::{params, Connection, Transaction};
use serenity::http::Http;

use super::BirthdayDateTime;
use crate::error::{SerenitySQLiteError, SerenitySQLiteResult};
use crate::BURDBOT_DB;

pub(crate) const RM_BDAY_ROLE_REASON: Option<&str> = Some("It's no longer their birthday");
pub(crate) const ADD_BDAY_ROLE_REASON: Option<&str> = Some("It's their birthday");

struct DatabaseRoleInfo {
    removal_list: Vec<(u64, u64, u64)>,
    addition_list: Vec<(u64, u64, u64)>,
}

pub async fn update_birthday_roles<T: AsRef<Http>>(http: T) -> SerenitySQLiteResult<()> {
    let user_role_info = update_bday_db_roles()?;
    let mut error_vector_option = None;
    let http = http.as_ref();

    for (user_id, guild_id, role_id) in user_role_info.removal_list {
        if let Err(error) = http.remove_member_role(guild_id, user_id, role_id, RM_BDAY_ROLE_REASON).await {
            let removal_errors = error_vector_option.get_or_insert(Vec::new());

            removal_errors.push(error);
        }
    }

    for (user_id, guild_id, role_id) in user_role_info.addition_list {
        if let Err(error) = http.add_member_role(guild_id, user_id, role_id, ADD_BDAY_ROLE_REASON).await {
            let addition_errors = error_vector_option.get_or_insert(Vec::new());

            addition_errors.push(error);
        }
    }

    match error_vector_option {
        None => Ok(()),
        Some(errors) => Err(SerenitySQLiteError::from(errors)),
    }
}

fn update_bday_db_roles() -> rusqlite::Result<DatabaseRoleInfo> {
    let mut connection = Connection::open(BURDBOT_DB)?;
    let transaction = connection.transaction()?;
    let date_time = get_date_time_to_use();
    let bdays_to_remove = get_and_delete_old_bdays(&transaction, date_time)?;
    let bdays_to_add = add_new_bdays(&transaction, date_time)?;

    transaction.commit()?;

    Ok(DatabaseRoleInfo { removal_list: bdays_to_remove, addition_list: bdays_to_add })
}

fn get_date_time_to_use() -> DateTime<Utc> {
    Utc::now() + Duration::hours(1)
}

fn get_and_delete_old_bdays(transaction: &Transaction, date_time: DateTime<Utc>) -> rusqlite::Result<Vec<(u64, u64, u64)>> {
    // get bdays from bday table that are 24 hrs in the past and set the end date to be 24 hrs from the start date.
    let mut query_info = Vec::new();

    let mut user_selection_statement = transaction.prepare(
        "
            SELECT 
                bday_user_list.user_id, 
                bday.guild_id,
                bday_role_list.role_id
            FROM bday_user_list
                INNER JOIN bday ON bday_user_list.user_id = bday.user_id
                INNER JOIN bday_role_list ON bday.guild_id = bday_role_list.guild_id
            WHERE bday_user_list.bday_over_date < ?;
    ",
    )?;

    let date_time_fmt = BirthdayDateTime::from(date_time);

    let rows =
        user_selection_statement.query_map([date_time_fmt], |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?, row.get::<_, u64>(2)?)))?;

    for row in rows {
        query_info.push(row.unwrap());
    }

    let user_deletion_statement = "
        DELETE FROM bday_user_list
        WHERE bday_over_date < ?;
    ";

    transaction.execute(user_deletion_statement, [date_time_fmt])?;

    Ok(query_info)
}

fn add_new_bdays(transaction: &Transaction, curr_date_time: DateTime<Utc>) -> rusqlite::Result<Vec<(u64, u64, u64)>> {
    let mut query_info = Vec::new();
    let mut user_selection_statement = if curr_date_time.month() != 1 || curr_date_time.day() != 1 {
        // If not Jan. 1
        transaction.prepare(
            "
                SELECT 
                    bday.user_id, 
                    bday.guild_id,
                    bday_role_list.role_id,
                    bday_date
                FROM bday
                    INNER JOIN bday_role_list ON bday.guild_id = bday_role_list.guild_id
                WHERE bday_date < ? AND bday_date > ?;
            ",
        )?
    } else {
        // If Jan. 1, we must ensure wrapping around is okay.
        transaction.prepare(
            "
                    SELECT 
                        bday.user_id, 
                        bday.guild_id,
                        bday_role_list.role_id,
                        bday_date
                    FROM bday
                        INNER JOIN bday_role_list ON bday.guild_id = bday_role_list.guild_id
                    WHERE bday_date < ? OR bday_date > ?;
                ",
        )?
    }; // 6-15-4 < 6-18-12 AND 6-15-4 > 6-17-11

    let earliest_date_time = curr_date_time - Duration::hours(25); // Checks 23 hrs or less away
    let curr_date_time_fmt = BirthdayDateTime::from(curr_date_time);
    let earliest_date_time_fmt = BirthdayDateTime::from(earliest_date_time);

    let rows = user_selection_statement.query_map([curr_date_time_fmt, earliest_date_time_fmt], |row| {
        Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?, row.get::<_, u64>(2)?, row.get::<_, BirthdayDateTime>(3)?))
    })?;

    let mut insertion_statement = transaction.prepare(
        "
        INSERT OR IGNORE INTO bday_user_list
            VALUES (?, ?);
    ",
    )?;

    for (user_id, guild_id, bday_role_id, bday_date) in rows.flatten() {
        let bday_date_time = bday_date.one_day_ahead();
        let rows_changed = insertion_statement.execute(params![user_id, bday_date_time])?;

        if rows_changed != 0 {
            query_info.push((user_id, guild_id, bday_role_id));
        }
    }

    Ok(query_info)
}
