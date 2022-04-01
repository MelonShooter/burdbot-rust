use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

use bimap::BiHashMap;
use serenity::model::id::{ChannelId, GuildId};
use serenity::model::prelude::VoiceState;
use serenity::prelude::*;
use songbird::error::JoinError;
use songbird::{CoreEvent, Songbird};

use crate::event_handler::BurdBotVoiceEventHandler;
use crate::IS_SESSION_TRACKER_ENABLED;

pub mod voice_handler;

const TARGET_GUILD_ID: u64 = 720900352018219039;
const TARGET_VOICE_CHANNEL_ID: u64 = 720900352597033053;

async fn join_target_voice_channel_with_context(context: &Context) {
    let manager = songbird::get(context).await.expect("Songbird Voice client placed in at initialisation.");

    join_target_voice_channel(&manager).await;
}

async fn join_target_voice_channel<T: AsRef<Songbird>>(manager: T) {
    let target_guild: GuildId = GuildId::from(TARGET_GUILD_ID);
    let target_voice_channel: ChannelId = ChannelId::from(TARGET_VOICE_CHANNEL_ID);
    let (handler_lock, conn_result) = manager.as_ref().join(target_guild, target_voice_channel).await;

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
        }
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
        if member.user.id.as_u64() != context.cache.current_user_id().await.as_u64() {
            return;
        }
    } else {
        return;
    }

    if new_state
        .channel_id
        .filter(|id| id == &ChannelId::from(TARGET_VOICE_CHANNEL_ID))
        .is_none()
    {
        join_target_voice_channel_with_context(context).await;
    }
}

pub async fn on_ready(context: &Context) {
    if IS_SESSION_TRACKER_ENABLED {
        join_target_voice_channel_with_context(context).await;
    }
}
