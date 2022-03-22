use log::{error, warn};
use rusqlite::{Connection, Error as RusqliteError, OptionalExtension, Transaction};
use serenity::client::Context;
use serenity::model::id::ChannelId;

use crate::commands::error_util::error::SerenitySQLiteError;
use crate::{commands, BURDBOT_DB};

use super::role_updater;

const NO_BIRTHDAY_SERVER_ROLE: &str = "This server has no birthday role currently.";

pub fn handle_update_birthday_roles_error(error: &SerenitySQLiteError) {
    if let SerenitySQLiteError::SerenityError(err) = error {
        warn!(
            "Error from SQLite or while adding or removing birthday roles. \
                            Couldn't add or remove roles from these people. \
                            Here is a vector of those errors.\
                            (Probably safe to ignore): {:?}",
            err
        );
    } else {
        error!("Error from SQLite or while adding or removing birthday roles: {:?}", error);
    }
}

pub async fn set_birthday_role(ctx: &Context, channel_id: &ChannelId, guild_id: u64, role_id: u64) -> Result<(), RusqliteError> {
    let connection = Connection::open(BURDBOT_DB)?;
    let insert_string = "
        INSERT OR REPLACE INTO bday_role_list
            VALUES(?, ?);
    ";

    connection.execute(insert_string, [guild_id, role_id])?;

    if let Err(error) = role_updater::update_birthday_roles(ctx.http.clone()).await {
        handle_update_birthday_roles_error(&error);
    }

    commands::send_message(ctx, channel_id, "The server's birthday role has been set.", "set_birthday_role").await;

    Ok(())
}

async fn is_actual_role(ctx: &Context, guild_id: u64, role_id: u64) -> bool {
    ctx.cache.role(guild_id, role_id).await.is_some()
}

fn get_birthday_role_id_conn(connection: &Connection, guild_id: u64) -> Result<Option<u64>, RusqliteError> {
    let select_string = "
        SELECT role_id
        FROM bday_role_list
        WHERE guild_id = ?;
    ";

    connection.query_row(select_string, [guild_id], |row| row.get::<_, u64>(0)).optional()
}

fn get_birthday_role_id_trans(connection: &Transaction, guild_id: u64) -> Result<Option<u64>, RusqliteError> {
    let select_string = "
        SELECT role_id
        FROM bday_role_list
        WHERE guild_id = ?;
    ";

    connection.query_row(select_string, [guild_id], |row| row.get::<_, u64>(0)).optional()
}

pub async fn get_birthday_role(ctx: &Context, channel_id: &ChannelId, guild_id: u64) -> Result<(), SerenitySQLiteError> {
    let role_id_option;

    {
        let connection = Connection::open(BURDBOT_DB)?;

        role_id_option = get_birthday_role_id_conn(&connection, guild_id)?;
    }

    if let Some(role_id) = role_id_option {
        if is_actual_role(ctx, guild_id, role_id).await {
            let message = format!("The server's current birthday role is {}", role_id);

            commands::send_message(ctx, channel_id, message, "get_birthday_role").await;
        } else {
            // The role no longer exists, clean it up.
            handle_db_birthday_removal(guild_id)?;
        }

        return Ok(());
    }

    commands::send_message(ctx, channel_id, NO_BIRTHDAY_SERVER_ROLE, "get_birthday_role").await;

    Ok(())
}

fn handle_db_birthday_removal(guild_id: u64) -> Result<Option<(Vec<u64>, u64)>, RusqliteError> {
    let mut connection = Connection::open(BURDBOT_DB)?;
    let transaction = connection.transaction()?;
    let user_id_query_string = "
        SELECT bday_user_list.user_id
        FROM bday_user_list
        INNER JOIN bday ON bday_user_list.user_id = bday.user_id
        WHERE bday.guild_id = ?;
    ";

    let remove_user_string = "
        DELETE FROM bday_user_list
        WHERE user_id IN 
        (   
            SELECT bday_user_list.user_id
            FROM bday_user_list
            INNER JOIN bday ON bday_user_list.user_id = bday.user_id
            WHERE bday.guild_id = ?
        );
    ";

    let remove_string = "
        DELETE FROM bday_role_list
        WHERE guild_id = ?;
    ";

    let mut deleted_users;
    let bday_role_id_option = get_birthday_role_id_trans(&transaction, guild_id)?;

    if bday_role_id_option.is_none() {
        return Ok(None);
    }

    let bday_role_id = bday_role_id_option.unwrap();

    {
        let mut rows_statement = transaction.prepare(user_id_query_string)?;
        let rows = rows_statement.query_map([guild_id], |row| row.get(0))?;
        deleted_users = Vec::new();

        for user_id_result in rows {
            let user_id = user_id_result.unwrap();

            deleted_users.push(user_id);
        }

        transaction.execute(remove_user_string, [guild_id])?;
    }

    transaction.execute(remove_string, [guild_id])?;
    transaction.commit()?;

    Ok(Some((deleted_users, bday_role_id)))
}

pub async fn remove_birthday_role(ctx: &Context, channel_id: &ChannelId, guild_id: u64) -> Result<(), SerenitySQLiteError> {
    let db_removal_result = handle_db_birthday_removal(guild_id)?;

    if db_removal_result.is_none() {
        commands::send_message(ctx, channel_id, NO_BIRTHDAY_SERVER_ROLE, "remove_birthday_role").await;

        return Ok(());
    }

    let (deleted_users, role_id) = db_removal_result.unwrap();

    for deleted_user in deleted_users {
        let mut error_vec = Vec::new();

        if let Err(error) = ctx.http.clone().remove_member_role(guild_id, deleted_user, role_id).await {
            error_vec.push(error);
        }

        if !error_vec.is_empty() {
            let error = SerenitySQLiteError::SerenityError(error_vec);

            handle_update_birthday_roles_error(&error);

            return Err(error);
        }
    }

    commands::send_message(ctx, channel_id, "The server's birthday role has been removed.", "set_birthday_role").await;

    Ok(())
}
