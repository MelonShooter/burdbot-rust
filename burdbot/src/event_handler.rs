use std::time::Duration;

use futures::join;
use log::info;
use serenity::async_trait;
use serenity::client::bridge::gateway::ChunkGuildFilter;
use serenity::client::{Context, EventHandler};
use serenity::model::channel::Message;
use serenity::model::guild::Guild;
use serenity::model::id::GuildId;
use serenity::model::prelude::{Ready, VoiceState};
use tokio::time;

use crate::commands::vocaroo;
use crate::logger;
use crate::spanish_english;

#[cfg(feature = "songbird")]
use {
    crate::session_tracker,
    bimap::BiHashMap,
    songbird::events::context_data::SpeakingUpdateData,
    songbird::model::payload::{ClientDisconnect, Speaking},
    songbird::{Event, EventContext, EventHandler as VoiceEventHandler},
    std::collections::HashMap,
    std::sync::{Arc, RwLock},
    std::time::Instant,
};

pub struct BurdBotEventHandler;

async fn chunk_guilds(ctx: &Context, guilds: &[GuildId]) {
    for guild in guilds {
        let get_unknowns = |g: &Guild| g.member_count - g.members.len() as u64;

        if let Some(left @ 1..) = ctx.cache.guild_field(guild, get_unknowns) {
            info!("Chunking {guild}... {left} users left.");
            ctx.shard.chunk_guild(*guild, None, ChunkGuildFilter::None, None);

            while let Some(1..) = ctx.cache.guild_field(guild, get_unknowns) {
                time::sleep(Duration::from_millis(300)).await;
            }

            info!("Finished chunking {guild}...");
        }
    }
}

#[async_trait]
impl EventHandler for BurdBotEventHandler {
    async fn ready(&self, context: Context, _ready: Ready) {
        crate::on_ready();

        #[cfg(feature = "songbird")]
        join!(session_tracker::on_ready(&context), vocaroo::on_ready(&context));

        #[cfg(not(feature = "songbird"))]
        vocaroo::on_ready(&context).await;
    }

    async fn message(&self, ctx: Context, new_message: Message) {
        join!(spanish_english::on_message_receive(&ctx, &new_message), vocaroo::on_message_received(&ctx, &new_message));
    }

    async fn cache_ready(&self, context: Context, guilds: Vec<GuildId>) {
        chunk_guilds(&context, guilds.as_slice()).await;

        crate::on_cache_ready(&context);

        join!(spanish_english::on_cache_ready(&context), logger::on_cache_ready(&context));
    }

    async fn voice_state_update(&self, context: Context, old_state: Option<VoiceState>, new_state: VoiceState) {
        #[cfg(feature = "songbird")]
        join!(
            session_tracker::on_voice_state_update(&new_state, &context),
            spanish_english::on_voice_state_update(old_state.as_ref(), &new_state, &context)
        );

        #[cfg(not(feature = "songbird"))]
        spanish_english::on_voice_state_update(old_state.as_ref(), &new_state, &context).await;
    }
}

#[cfg(feature = "songbird")]
pub struct BurdBotVoiceEventHandler {
    pub ssrc_to_user_id: Arc<RwLock<BiHashMap<u32, u64>>>,
    pub user_id_to_start: Arc<RwLock<HashMap<u64, Instant>>>,
}

#[cfg(feature = "songbird")]
impl BurdBotVoiceEventHandler {
    pub fn new(ssrc_to_user_id_map: Arc<RwLock<BiHashMap<u32, u64>>>, user_id_to_start_map: Arc<RwLock<HashMap<u64, Instant>>>) -> Self {
        Self { ssrc_to_user_id: ssrc_to_user_id_map, user_id_to_start: user_id_to_start_map }
    }
}

#[cfg(feature = "songbird")]
#[async_trait]
impl VoiceEventHandler for BurdBotVoiceEventHandler {
    async fn act(&self, context: &EventContext<'_>) -> Option<Event> {
        match context {
            EventContext::SpeakingStateUpdate(Speaking { ssrc, user_id, .. }) => {
                session_tracker::on_speaking_state_update(self, user_id, *ssrc);
            },

            EventContext::SpeakingUpdate(SpeakingUpdateData { speaking, ssrc, .. }) => {
                session_tracker::on_speaking_update(self, *speaking, *ssrc);
            },

            EventContext::ClientDisconnect(ClientDisconnect { user_id }) => {
                session_tracker::on_client_disconnect(self, *user_id);
            },

            _ => {},
        }

        None
    }
}
