use log::error;
use rusqlite::Connection;
use rusqlite::Error;
use songbird::model::id::UserId;
use std::time::{Duration, Instant};

use crate::events::BurdBotVoiceEventHandler;

fn write_duration(user_id: u64, duration: Duration) -> Result<usize, Error> {
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

pub fn on_speaking_state_update(event_handler: &BurdBotVoiceEventHandler, user_id: &Option<UserId>, ssrc: u32) {
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
