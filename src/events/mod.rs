use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use bimap::BiHashMap;
use serenity::async_trait;
use serenity::client::{Context, EventHandler};
use serenity::model::id::GuildId;
use serenity::model::prelude::{Ready, VoiceState};
use songbird::model::payload::{ClientDisconnect, Speaking};
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler};

use crate::commands::user_search_engine;
use crate::session_tracker::{self, voice_handler};

pub struct BurdBotEventHandler;

#[async_trait]
impl EventHandler for BurdBotEventHandler {
    async fn ready(&self, context: Context, _ready: Ready) {
        crate::on_ready();
        session_tracker::on_ready(context).await;
    }

    async fn cache_ready(&self, context: Context, _guilds: Vec<GuildId>) {
        user_search_engine::on_cache_ready(context).await;
    }

    async fn voice_state_update(&self, context: Context, _guild_id: Option<GuildId>, _old_state: Option<VoiceState>, new_state: VoiceState) {
        session_tracker::on_voice_state_update(new_state, context).await;
    }
}

pub struct BurdBotVoiceEventHandler {
    pub ssrc_to_user_id: Arc<RwLock<BiHashMap<u32, u64>>>,
    pub user_id_to_start: Arc<RwLock<HashMap<u64, Instant>>>,
}

impl BurdBotVoiceEventHandler {
    pub fn new(ssrc_to_user_id_map: Arc<RwLock<BiHashMap<u32, u64>>>, user_id_to_start_map: Arc<RwLock<HashMap<u64, Instant>>>) -> Self {
        Self {
            ssrc_to_user_id: ssrc_to_user_id_map,
            user_id_to_start: user_id_to_start_map,
        }
    }
}

#[async_trait]
impl VoiceEventHandler for BurdBotVoiceEventHandler {
    async fn act(&self, context: &EventContext<'_>) -> Option<Event> {
        match context {
            EventContext::SpeakingStateUpdate(Speaking { ssrc, user_id, ..}) => {
                voice_handler::on_speaking_state_update(self, user_id, ssrc);
            }

            EventContext::SpeakingUpdate { speaking, ssrc } => {
                voice_handler::on_speaking_update(self, speaking, ssrc);
            }

            EventContext::ClientDisconnect(ClientDisconnect { user_id }) => {
                voice_handler::on_client_disconnect(self, user_id);
            }

            _ => {}
        }

        None
    }
}