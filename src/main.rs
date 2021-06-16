mod birthday_tracker;
mod commands;
mod secret;
mod session_tracker;
mod util;
mod events;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::{Timelike, Utc};
use events::BurdBotEventHandler;
use log::{LevelFilter, error, info};
use rusqlite::Connection;
use serenity::client::bridge::gateway::GatewayIntents;
use serenity::client::Context;
use serenity::framework::standard::macros::hook;
use serenity::framework::StandardFramework;
use serenity::model::channel::Message;
use serenity::model::id::UserId;
use serenity::{CacheAndHttp, Client};
use simple_logger::SimpleLogger;
use songbird::driver::{Config, DecodeMode};
use songbird::{SerenityInit, Songbird};
use tokio::time;

pub const BURDBOT_DB: &str = "burdbot.db";
pub const DELIBURD_ID: u64 = 367538590520967181;

fn create_sql_tables() {
    let mut connection = Connection::open(BURDBOT_DB).unwrap();
    let transaction = connection.transaction().unwrap();
    let table_statements = "
        CREATE TABLE IF NOT EXISTS times (
            user_id INTEGER PRIMARY KEY,
            time INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS bday (
            user_id INTEGER PRIMARY KEY,
            guild_id INTEGER NOT NULL,
            bday_date TEXT NOT NULL,
            FOREIGN KEY(guild_id) REFERENCES bday_role_list(guild_id)
        );

        CREATE TABLE IF NOT EXISTS bday_role_list (
            guild_id INTEGER PRIMARY KEY,
            role_id INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS bday_user_list (
            user_id INTEGER NOT NULL,
            bday_over_date TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS bday_date_index
            on bday (bday_date);

        CREATE INDEX IF NOT EXISTS bday_over_date_index
            on bday_user_list (bday_over_date);
    ";

    transaction.execute_batch(table_statements).unwrap();
    transaction.commit().unwrap();
}

fn setup_birthday_tracker(cache_and_http: Arc<CacheAndHttp>) {
    tokio::spawn(async move {
        loop {
            let seconds = 3600 - Utc::now().num_seconds_from_midnight() % 3600; // Get time in seconds until next hour.
            let sleep_time = Duration::from_secs(seconds.into());

            time::sleep(sleep_time).await;

            if let Err(error) = birthday_tracker::update_birthday_roles(&cache_and_http).await {
                error!("Error from SQLite or while removing birthday roles: {:?}", error);
            }
        }
    });
}

#[hook]
async fn on_unrecognized_command(ctx: &Context, msg: &Message, _: &str) {
    commands::error_util::unknown_command_message(ctx, &msg.channel_id).await;
}

#[tokio::main]
async fn main() {
    SimpleLogger::new().with_level(LevelFilter::Warn)
            .with_module_level("burdbot", LevelFilter::Info)
            .init().unwrap();

    let mut owners_set = HashSet::with_capacity(1);
    owners_set.insert(UserId::from(367538590520967181));

    let framework = StandardFramework::new()
        .configure(|c| c.prefix(",,,").with_whitespace(true).case_insensitivity(true).owners(owners_set))
        .bucket("default", |bucket| bucket.delay(1).limit(5).time_span(10))
        .await
        .bucket("intense", |bucket| bucket.delay(2).limit(2).time_span(10))
        .await
        .bucket("db_operations", |bucket| bucket.delay(3).limit(3).time_span(10))
        .await
        .unrecognised_command(on_unrecognized_command)
        .help(&commands::HELP)
        .group(&commands::BIRTHDAY_GROUP);

    let songbird = Songbird::serenity();

    songbird.set_config(Config::default().decode_mode(DecodeMode::Decode));

    create_sql_tables();

    let mut client = Client::builder(secret::TOKEN)
        .framework(framework)
        .intents(GatewayIntents::all())
        .event_handler(BurdBotEventHandler)
        .register_songbird_with(songbird.into())
        .await
        .expect("Couldn't build client.");

    let cache = client.cache_and_http.clone();

    setup_birthday_tracker(cache);

    client.start().await.expect("Couldn't start client.");
}

pub fn on_ready() {
    info!("BurdBot loaded");
}