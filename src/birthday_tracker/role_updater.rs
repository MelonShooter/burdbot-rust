use chrono::{DateTime, Datelike, Duration, Utc};
use rusqlite::{params, Connection};
use rusqlite::{Error as SQLiteError, Transaction};
use serenity::CacheAndHttp;

use crate::commands::error_util::error::SerenitySQLiteError;
use crate::BURDBOT_DB;

use super::BirthdayDateTime;

struct DatabaseRoleInfo {
    removal_list: Vec<(u64, u64, u64)>,
    addition_list: Vec<(u64, u64, u64)>,
}

pub async fn update_birthday_roles(cache_and_http: &CacheAndHttp) -> Result<(), SerenitySQLiteError> {
    let user_role_info = update_bday_db_roles()?;

    let http = cache_and_http.http.clone();
    let mut error_vector_option = None;

    for (user_id, guild_id, role_id) in user_role_info.removal_list {
        if let Err(error) = http.remove_member_role(guild_id, user_id, role_id).await {
            let removal_errors = error_vector_option.get_or_insert(Vec::new());

            removal_errors.push(error);
        }
    }

    for (user_id, guild_id, role_id) in user_role_info.addition_list {
        if let Err(error) = http.add_member_role(guild_id, user_id, role_id).await {
            let addition_errors = error_vector_option.get_or_insert(Vec::new());

            addition_errors.push(error);
        }
    }

    match error_vector_option {
        None => Ok(()),
        Some(errors) => Err(SerenitySQLiteError::from(errors)),
    }
}

fn update_bday_db_roles() -> Result<DatabaseRoleInfo, SQLiteError> {
    let mut connection = Connection::open(BURDBOT_DB)?;
    let transaction = connection.transaction()?;
    let date_time = get_date_time_to_use();
    let bdays_to_remove = get_and_delete_old_bdays(&transaction, date_time)?;
    let bdays_to_add = add_new_bdays(&&transaction, date_time)?;

    transaction.commit()?;

    Ok(DatabaseRoleInfo {
        removal_list: bdays_to_remove,
        addition_list: bdays_to_add,
    })
}

fn get_date_time_to_use() -> DateTime<Utc> {
    Utc::now() + Duration::hours(1)
}

fn get_and_delete_old_bdays(transaction: &Transaction, date_time: DateTime<Utc>) -> Result<Vec<(u64, u64, u64)>, SQLiteError> {
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

    let rows = user_selection_statement.query_map([date_time_fmt], |row| {
        Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?, row.get::<_, u64>(2)?))
    })?;

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

fn add_new_bdays(transaction: &Transaction, curr_date_time: DateTime<Utc>) -> Result<Vec<(u64, u64, u64)>, SQLiteError> {
    let mut query_info = Vec::new();
    let mut user_selection_statement;

    if curr_date_time.month() != 1 || curr_date_time.day() != 1 {
        // If not Jan. 1
        user_selection_statement = transaction.prepare(
            "
                SELECT 
                    bday.user_id, 
                    bday.guild_id,
                    bday_role_list.role_id
                    bday_date
                FROM bday
                    INNER JOIN bday_role_list ON bday.user_id = bday_role_list.user_id
                WHERE bday_date < ? AND bday_date > ?;
            ",
        )?;
    } else {
        // If Jan. 1, we must ensure wrapping around is okay.
        user_selection_statement = transaction.prepare(
            "
                    SELECT 
                        bday.user_id, 
                        bday.guild_id,
                        bday_role_list.role_id
                        bday_date
                    FROM bday
                        INNER JOIN bday_role_list ON bday.user_id = bday_role_list.user_id
                    WHERE bday_date < ? OR bday_date > ?;
                ",
        )?;
    }

    let earliest_date_time = curr_date_time - Duration::hours(25); // Checks 23 hrs or less away
    let curr_date_time_fmt = BirthdayDateTime::from(curr_date_time);
    let earliest_date_time_fmt = BirthdayDateTime::from(earliest_date_time);

    let rows = user_selection_statement.query_map([curr_date_time_fmt, earliest_date_time_fmt], |row| {
        Ok((
            row.get::<_, u64>(0)?,
            row.get::<_, u64>(1)?,
            row.get::<_, u64>(2)?,
            row.get::<_, DateTime<Utc>>(3)?,
        ))
    })?;

    let mut insertion_statement = transaction.prepare(
        "
        INSERT OR IGNORE INTO bday_user_list
            VALUES (?, ?);
    ",
    )?;

    for row in rows {
        let bday_data = row.unwrap();
        let bday_over = bday_data.3 + Duration::days(1);
        let bday_over_fmt = BirthdayDateTime::from(bday_over);
        let rows_changed = insertion_statement.execute(params![bday_data.0, bday_over_fmt])?;

        if rows_changed != 0 {
            query_info.push((bday_data.0, bday_data.1, bday_data.2));
        }
    }

    Ok(query_info)
}
