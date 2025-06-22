use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;
use std::time::Instant;

use bimap::BiHashMap;
use log::error;
use rusqlite::Connection;
use serenity::model::id::{ChannelId, GuildId};
use serenity::model::prelude::VoiceState;
use serenity::prelude::*;
use songbird::error::JoinError;
use songbird::model::id::UserId;
use songbird::{CoreEvent, Songbird};

use crate::IS_SESSION_TRACKER_ENABLED;
use crate::event_handler::BurdBotVoiceEventHandler;

const TARGET_GUILD_ID: u64 = 720900352018219039;
const TARGET_VOICE_CHANNEL_ID: u64 = 720900352597033053;

async fn join_target_voice_channel_with_context(context: &Context) {
    let manager =
        songbird::get(context).await.expect("Songbird Voice client placed in at initialisation.");

    join_target_voice_channel(&manager).await;
}

async fn join_target_voice_channel<T: AsRef<Songbird>>(manager: T) {
    let target_guild: GuildId = GuildId::from(TARGET_GUILD_ID);
    let target_voice_channel: ChannelId = ChannelId::from(TARGET_VOICE_CHANNEL_ID);
    let (handler_lock, conn_result) =
        manager.as_ref().join(target_guild, target_voice_channel).await;

    match conn_result {
        Ok(()) => {
            let mut handler = handler_lock.lock().await;
            let ssrc_user_to_id = Arc::new(RwLock::new(BiHashMap::new()));
            let user_id_to_start = Arc::new(RwLock::new(HashMap::new()));

            handler.remove_all_global_events();

            handler.add_global_event(
                CoreEvent::SpeakingStateUpdate.into(),
                BurdBotVoiceEventHandler::new(ssrc_user_to_id.clone(), user_id_to_start.clone()),
            );

            handler.add_global_event(
                CoreEvent::SpeakingUpdate.into(),
                BurdBotVoiceEventHandler::new(ssrc_user_to_id.clone(), user_id_to_start.clone()),
            );

            handler.add_global_event(
                CoreEvent::ClientDisconnect.into(),
                BurdBotVoiceEventHandler::new(ssrc_user_to_id, user_id_to_start),
            );
        },
        Err(err) => match err {
            JoinError::Driver(_) => (),
            _ => log::error!("Failed to join target voice channel!"),
        },
    }
}

pub async fn on_voice_state_update(new_state: &VoiceState, context: &Context) {
    if !IS_SESSION_TRACKER_ENABLED {
        return;
    }

    if let Some(member) = &new_state.member {
        if member.user.id != context.cache.current_user_id() {
            return;
        }
    } else {
        return;
    }

    if new_state.channel_id.filter(|id| id == &ChannelId::from(TARGET_VOICE_CHANNEL_ID)).is_none() {
        join_target_voice_channel_with_context(context).await;
    }
}

pub async fn on_ready(context: &Context) {
    if IS_SESSION_TRACKER_ENABLED {
        join_target_voice_channel_with_context(context).await;
    }
}

fn write_duration(user_id: u64, duration: Duration) -> rusqlite::Result<usize> {
    let user_id_signed = user_id as i64;
    let duration_seconds = duration.as_secs() as i64;
    let connection = Connection::open("times.db")?;

    let statement_str = "
    INSERT INTO times
        VALUES(?, ?)
        ON CONFLICT(user_id) DO UPDATE SET
            time = time + excluded.time;
    ";

    connection.execute(statement_str, [user_id_signed, duration_seconds])
}

fn write_duration_with_error(start_time: &Instant, id: u64) {
    let duration = start_time.elapsed();

    if let Err(error) = write_duration(id, duration) {
        error!("Error while writing duration to database: {:?}", error);
    }
}

pub fn on_speaking_state_update(
    event_handler: &BurdBotVoiceEventHandler, user_id: &Option<UserId>, ssrc: u32,
) {
    if let Some(id) = user_id {
        let mut ssrc_to_user_id = event_handler.ssrc_to_user_id.write().unwrap();
        let mut user_id_to_start = event_handler.user_id_to_start.write().unwrap();
        let user_id = id.0;

        ssrc_to_user_id.insert(ssrc, user_id);
        user_id_to_start.entry(user_id).or_insert_with(Instant::now);
    }
}

pub fn on_speaking_update(event_handler: &BurdBotVoiceEventHandler, speaking: bool, ssrc: u32) {
    let mut user_id_to_start = event_handler.user_id_to_start.write().unwrap();
    let ssrc_to_user_id = event_handler.ssrc_to_user_id.read().unwrap();
    let user_id = ssrc_to_user_id.get_by_left(&ssrc);

    if let Some(&id) = user_id {
        if speaking {
            user_id_to_start.entry(id).or_insert_with(Instant::now);
        } else if let Some(start_time) = user_id_to_start.get(&id) {
            write_duration_with_error(start_time, id);
            user_id_to_start.remove(&id);
        }
    }
}

pub fn on_client_disconnect(event_handler: &BurdBotVoiceEventHandler, user_id: UserId) {
    let mut user_id_to_start = event_handler.user_id_to_start.write().unwrap();
    let mut ssrc_to_user_id = event_handler.ssrc_to_user_id.write().unwrap();
    let user_id_number = user_id.0;

    if let Some(start_time) = user_id_to_start.get(&user_id_number) {
        write_duration_with_error(start_time, user_id_number);
        user_id_to_start.remove(&user_id_number);
    }

    ssrc_to_user_id.remove_by_right(&user_id_number);
}
