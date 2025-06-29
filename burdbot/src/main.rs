// Temporary until all of this is moved to poise
#![allow(deprecated)]

pub mod argument_parser;
pub mod forvo;
pub mod id_search_engine;
pub mod vocaroo;

mod birthday_tracker;
mod commands;
mod error;
mod event_handler;
mod image_checker;
mod logger;
mod spanish_english;
mod util;

#[cfg(feature = "songbird")]
mod session_tracker;

use async_ctrlc::CtrlC;
use chrono::{Timelike, Utc};
use event_handler::BurdBotEventHandler;
use log::{LevelFilter, info, warn};
use logger::{DiscordLogger, LogSender};
use rusqlite::Connection;
use serenity::Client;
use serenity::all::ShardManager;
use serenity::all::standard::{BucketBuilder, Configuration};
use serenity::client::Context;
use serenity::framework::StandardFramework;
use serenity::framework::standard::CommandResult;
use serenity::framework::standard::macros::hook;
use serenity::http::Http;
use serenity::model::channel::Message;
use serenity::model::id::UserId;
use serenity::prelude::GatewayIntents;
use simplelog::{CombinedLogger, ConfigBuilder, WriteLogger};
use std::collections::HashSet;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::time;

#[cfg(feature = "songbird")]
use {
    songbird::driver::DecodeMode,
    songbird::{Config, SerenityInit},
};

#[cfg(feature = "songbird")]
pub(crate) const IS_SESSION_TRACKER_ENABLED: bool = false;

pub(crate) const BURDBOT_TOKEN_NAME: &str = "BURDBOT_TOKEN";
pub(crate) const BURDBOT_DB: &str = "burdbot.db";
pub(crate) const DELIBURD_ID: u64 = 367538590520967181;
pub(crate) const PREFIX: &str = ",";
const BURDBOT_LOGGER_BUFFER_SIZE: usize = (1 << 10) * 32; // 32KB
const DEFAULT_LOGGER_BUFFER_SIZE: usize = 1 << 10; // 1KB
const LOGGER_WRITE_COOLDOWN: Duration = Duration::from_secs(15);
const LOGGER_FAILED_FILE: &str = "failed-to-send-logs.txt";
const LOGGER_FILE_NAME: &str = "log.txt";
const RETRY_CONNECTION_INTERVAL: u64 = 30;

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

        CREATE TABLE IF NOT EXISTS bday_user_list (
            user_id INTEGER,
            bday_over_date TEXT NOT NULL,
            FOREIGN KEY (user_id) REFERENCES bday(user_id) ON DELETE CASCADE
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

        CREATE TABLE IF NOT EXISTS fxhash_image_checksums (
            link_reference TEXT PRIMARY KEY,
            width INTEGER NOT NULL,
            height INTEGER NOT NULL,
            description TEXT NOT NULL,
            hash BLOB NOT NULL,
            hash_type INTEGER NOT NULL,
            guild_id INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS fxhash_checksum_index
            on fxhash_image_checksums (guild_id, width, height);

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

pub(crate) fn on_cache_ready(ctx: &Context) {
    setup_birthday_tracker(ctx.http.clone());
}

fn setup_birthday_tracker(http: Arc<Http>) {
    tokio::spawn(async move {
        loop {
            let seconds = 3600 - Utc::now().num_seconds_from_midnight() % 3600; // Get time in seconds until next hour.
            let sleep_time = Duration::from_secs(seconds.into());

            time::sleep(sleep_time).await;

            if let Err(error) = birthday_tracker::update_birthday_roles(&http).await {
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
    info!("Result of {}{}: {:?}", PREFIX, cmd, result);
}

async fn on_terminate(
    shard_manager: Arc<ShardManager>, mut log_sender_mpsc_recv: UnboundedReceiver<LogSender>,
) {
    // Flush the logger so that all logs are sent.
    info!("Flushed logger. Terminating bot...");

    log::logger().flush();

    log_sender_mpsc_recv.close();

    match log_sender_mpsc_recv.recv().await {
        Some(sender) => sender.send().await,
        None => eprintln!(
            "Failed to flush the logger. No log sender was sent.\nContinuing termination..."
        ),
    };

    shard_manager.shutdown_all().await;
}

#[tokio::main]
async fn main() {
    let mut owners_set = HashSet::with_capacity(1);
    owners_set.insert(UserId::from(367538590520967181));

    let framework = StandardFramework::new()
        .bucket("default", BucketBuilder::new_user().delay(1).limit(5).time_span(10))
        .await
        .bucket("intense", BucketBuilder::new_user().delay(2).limit(2).time_span(10))
        .await
        .bucket("db_operations", BucketBuilder::new_user().delay(3).limit(3).time_span(10))
        .await
        .bucket("very_intense", BucketBuilder::new_user().delay(10).limit(4).time_span(600))
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

    framework.configure(
        Configuration::new()
            .prefix(PREFIX)
            .with_whitespace(true)
            .case_insensitivity(true)
            .owners(owners_set),
    );

    #[cfg(feature = "songbird")]
    let songbird_config = Config::default().decode_mode(DecodeMode::Decode);

    create_sql_tables();

    let token = env::var(BURDBOT_TOKEN_NAME).unwrap();

    #[cfg(feature = "songbird")]
    let mut client = Client::builder(token, GatewayIntents::all())
        .framework(framework)
        .event_handler(BurdBotEventHandler)
        .register_songbird_from_config(songbird_config)
        .await
        .expect("Couldn't build client.");

    #[cfg(not(feature = "songbird"))]
    let mut client = Client::builder(token, GatewayIntents::all())
        .framework(framework)
        .event_handler(BurdBotEventHandler)
        .await
        .expect("Couldn't build client.");

    let cache = &client.cache;
    let http = &client.http;
    let shard_manager = client.shard_manager.clone();
    let (log_sender_mpsc_send, log_sender_mpsc_recv) = mpsc::unbounded_channel();
    let burdbot_log_config = ConfigBuilder::new()
        .set_max_level(LevelFilter::Error)
        .set_time_level(LevelFilter::Error)
        .set_location_level(LevelFilter::Off)
        .set_target_level(LevelFilter::Error)
        .set_thread_level(LevelFilter::Off)
        .add_filter_allow_str("burdbot")
        .build();
    let default_log_config = ConfigBuilder::new()
        .set_max_level(LevelFilter::Error)
        .set_time_level(LevelFilter::Error)
        .set_location_level(LevelFilter::Error)
        .set_target_level(LevelFilter::Error)
        .set_thread_level(LevelFilter::Off)
        .add_filter_ignore_str("burdbot")
        .build();

    // In debug mode, the debug logs will appear
    #[cfg(debug_assertions)]
    let log_level = LevelFilter::Debug;
    #[cfg(not(debug_assertions))]
    let log_level = LevelFilter::Info;

    CombinedLogger::init(vec![
        WriteLogger::new(
            log_level,
            burdbot_log_config,
            DiscordLogger::new(
                cache.clone(),
                http.clone(),
                BURDBOT_LOGGER_BUFFER_SIZE,
                LOGGER_FAILED_FILE,
                LOGGER_FILE_NAME,
                LOGGER_WRITE_COOLDOWN,
                log_sender_mpsc_send.clone(),
            ),
        ),
        WriteLogger::new(
            LevelFilter::Warn,
            default_log_config,
            DiscordLogger::new(
                cache.clone(),
                http.clone(),
                DEFAULT_LOGGER_BUFFER_SIZE,
                LOGGER_FAILED_FILE,
                LOGGER_FILE_NAME,
                LOGGER_WRITE_COOLDOWN,
                log_sender_mpsc_send,
            ),
        ),
    ])
    .expect("Unable to intialize logger.");

    tokio::spawn(async move {
        CtrlC::new().expect("Failed to create ctrl + c handler.").await;

        on_terminate(shard_manager, log_sender_mpsc_recv).await;
    });

    while let Err(err) = client.start().await {
        warn!(
            "Error encountered starting Discord bot client: {err}\nRetrying in {RETRY_CONNECTION_INTERVAL} seconds."
        );

        time::sleep(Duration::from_secs(RETRY_CONNECTION_INTERVAL)).await;
    }
}

pub(crate) fn on_ready() {
    info!("BurdBot loaded");
}
