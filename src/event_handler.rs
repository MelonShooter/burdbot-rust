use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use bimap::BiHashMap;
use futures::join;
use serenity::async_trait;
use serenity::client::{Context, EventHandler};
use serenity::model::channel::Message;
use serenity::model::id::GuildId;
use serenity::model::prelude::{Ready, VoiceState};
use songbird::model::payload::{ClientDisconnect, Speaking};
use songbird::{Event, EventContext, EventHandler as VoiceEventHandler};

use crate::commands::vocaroo;
use crate::logger;
use crate::session_tracker::{self, voice_handler};
use crate::spanish_english;

pub struct BurdBotEventHandler;

#[async_trait]
impl EventHandler for BurdBotEventHandler {
    async fn ready(&self, context: Context, _ready: Ready) {
        crate::on_ready();

        join!(session_tracker::on_ready(&context), vocaroo::on_ready(&context));
    }

    async fn message(&self, ctx: Context, new_message: Message) {
        join!(
            spanish_english::on_message_receive(&ctx, &new_message),
            vocaroo::on_message_received(&ctx, &new_message)
        );
    }

    async fn cache_ready(&self, context: Context, _guilds: Vec<GuildId>) {
        join!(spanish_english::on_cache_ready(&context), logger::on_cache_ready(&context));
    }

    async fn voice_state_update(&self, context: Context, _guild_id: Option<GuildId>, old_state: Option<VoiceState>, new_state: VoiceState) {
        join!(
            session_tracker::on_voice_state_update(&new_state, &context),
            spanish_english::on_voice_state_update(old_state.as_ref(), &new_state, &context)
        );
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
            EventContext::SpeakingStateUpdate(Speaking { ssrc, user_id, .. }) => {
                voice_handler::on_speaking_state_update(self, user_id, *ssrc);
            }

            EventContext::SpeakingUpdate { speaking, ssrc } => {
                voice_handler::on_speaking_update(self, *speaking, *ssrc);
            }

            EventContext::ClientDisconnect(ClientDisconnect { user_id }) => {
                voice_handler::on_client_disconnect(self, *user_id);
            }

            _ => {}
        }

        None
    }
}
