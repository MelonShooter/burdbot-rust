mod birthday_tracker;
mod commands;
mod custom;
mod events;
mod logger;
mod secret;
mod session_tracker;
mod util;

use async_ctrlc::CtrlC;
use chrono::{Timelike, Utc};
use events::BurdBotEventHandler;
use log::{debug, info, LevelFilter};
use logger::DiscordLogger;
use rusqlite::Connection;
use serenity::client::bridge::gateway::{GatewayIntents, ShardManager};
use serenity::client::Context;
use serenity::framework::standard::macros::hook;
use serenity::framework::standard::CommandResult;
use serenity::framework::StandardFramework;
use serenity::model::channel::Message;
use serenity::model::id::UserId;
use serenity::prelude::Mutex;
use serenity::{CacheAndHttp, Client};
use simplelog::{CombinedLogger, ConfigBuilder, WriteLogger};
use songbird::driver::{Config, DecodeMode};
use songbird::{SerenityInit, Songbird};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

pub const BURDBOT_DB: &str = "burdbot.db";
pub const DELIBURD_ID: u64 = 367538590520967181;
pub const PREFIX: &str = ",";
const BURDBOT_LOGGER_BUFFER_SIZE: usize = (1 << 10) * 32; // 32KB
const DEFAULT_LOGGER_BUFFER_SIZE: usize = (1 << 10) * 1; // 1KB
const LOGGER_WRITE_COOLDOWN: Duration = Duration::from_secs(15);

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
            bday_date TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS bday_role_list (
            guild_id INTEGER PRIMARY KEY,
            role_id INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS staff_logs (
            user_id INTEGER NOT NULL,
            entry_id INTEGER NOT NULL,
            original_link TEXT NOT NULL,
            last_edited_link TEXT,
            reason TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS vocaroo_enabled (
            guild_id INTEGER PRIMARY KEY
        );

        CREATE INDEX IF NOT EXISTS bday_date_index
            on bday (bday_date);

        CREATE INDEX IF NOT EXISTS bday_over_date_index
            on bday_user_list (bday_over_date);

        CREATE INDEX IF NOT EXISTS staff_log_index
            on staff_logs (user_id);
    ";

    transaction.execute_batch(table_statements).unwrap();
    transaction.commit().unwrap();
}

fn setup_birthday_tracker<T: AsRef<CacheAndHttp>>(cache_and_http: T) {
    let cache_and_http = cache_and_http.as_ref();
    let http = cache_and_http.http.clone();

    tokio::spawn(async move {
        loop {
            let seconds = 3600 - Utc::now().num_seconds_from_midnight() % 3600; // Get time in seconds until next hour.
            let sleep_time = Duration::from_secs(seconds.into());

            time::sleep(sleep_time).await;

            // Wait until all members are fetched
            /*loop {
                if cache_and_http.cache.unknown_members().await == 0 {
                    break;
                }

                info!("Still have unknown members. Waiting 10 seconds before updating birthday roles.");
                time::sleep(Duration::from_secs(10)).await;
            }*/

            if let Err(error) = birthday_tracker::update_birthday_roles(http.clone()).await {
                birthday_tracker::handle_update_birthday_roles_error(&error);
            }
        }
    });
}

/*#[hook]
async fn on_unrecognized_command(ctx: &Context, msg: &Message, _: &str) {
    commands::error_util::unknown_command_message(ctx, &msg.channel_id).await; // uncomment function for this to work in error_util.rs
}*/

#[hook]
async fn on_post_command(_: &Context, _: &Message, cmd: &str, result: CommandResult) {
    debug!("Result of {}{}: {:?}", PREFIX, cmd, result);
}

async fn on_terminate(shard_manager: Arc<Mutex<ShardManager>>) {
    shard_manager.lock().await.shutdown_all().await;
}

#[tokio::main]
async fn main() {
    let mut owners_set = HashSet::with_capacity(1);
    owners_set.insert(UserId::from(367538590520967181));

    let framework = StandardFramework::new()
        .configure(|c| c.prefix(PREFIX).with_whitespace(true).case_insensitivity(true).owners(owners_set))
        .bucket("default", |bucket| bucket.delay(1).limit(5).time_span(10))
        .await
        .bucket("intense", |bucket| bucket.delay(2).limit(2).time_span(10))
        .await
        .bucket("db_operations", |bucket| bucket.delay(3).limit(3).time_span(10))
        .await
        .bucket("very_intense", |bucket| bucket.delay(10).limit(4).time_span(600))
        .await
        //.unrecognised_command(on_unrecognized_command)
        .after(on_post_command)
        .help(&commands::HELP)
        .group(&commands::BIRTHDAY_GROUP)
        .group(&commands::EASTEREGG_GROUP)
        .group(&commands::VOCAROO_GROUP)
        .group(&commands::CUSTOM_GROUP)
        .group(&commands::ADMINISTRATIVE_GROUP)
        .group(&commands::LANGUAGE_GROUP);

    let songbird = Songbird::serenity();

    songbird.set_config(Config::default().decode_mode(DecodeMode::Decode));

    create_sql_tables();

    let mut client = Client::builder(secret::TOKEN)
        .framework(framework)
        .intents(GatewayIntents::all())
        .event_handler(BurdBotEventHandler)
        .register_songbird_with(songbird)
        .await
        .expect("Couldn't build client.");

    let cache_and_http = &client.cache_and_http;
    let shard_manager = client.shard_manager.clone();
    let burdbot_log_config = ConfigBuilder::new()
        .set_max_level(LevelFilter::Error)
        .set_time_level(LevelFilter::Off)
        .set_location_level(LevelFilter::Error)
        .set_target_level(LevelFilter::Error)
        .set_thread_level(LevelFilter::Off)
        .add_filter_allow_str("burdbot")
        .build();
    let default_log_config = ConfigBuilder::new()
        .set_max_level(LevelFilter::Error)
        .set_time_level(LevelFilter::Off)
        .set_location_level(LevelFilter::Error)
        .set_target_level(LevelFilter::Error)
        .set_thread_level(LevelFilter::Off)
        .add_filter_ignore_str("burdbot")
        .build();

    CombinedLogger::init(vec![
        WriteLogger::new(
            LevelFilter::Info,
            burdbot_log_config,
            DiscordLogger::new(cache_and_http, BURDBOT_LOGGER_BUFFER_SIZE, LOGGER_WRITE_COOLDOWN),
        ),
        WriteLogger::new(
            LevelFilter::Warn,
            default_log_config,
            DiscordLogger::new(cache_and_http, DEFAULT_LOGGER_BUFFER_SIZE, LOGGER_WRITE_COOLDOWN),
        ),
    ])
    .expect("Unable to intialize logger.");

    setup_birthday_tracker(cache_and_http);

    tokio::spawn(async move {
        CtrlC::new().expect("Failed to create ctrl + c handler.").await;

        on_terminate(shard_manager).await;
    });

    client.start().await.expect("Couldn't start client.");
}

pub fn on_ready() {
    info!("BurdBot loaded");
}
