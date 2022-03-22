use std::collections::HashMap;

use serenity::client::Context;
use serenity::model::guild::{Guild, Member};
use serenity::model::id::GuildId;
use serenity::prelude::TypeMapKey;
use simsearch::{SearchOptions, SimSearch};

pub struct UserSearchEngine;

impl TypeMapKey for UserSearchEngine {
    type Value = HashMap<u64, SimSearch<u64>>;
}

fn add_member_to_search_engine(nick_option: Option<&str>, search_engine: &mut SimSearch<u64>, id: u64, name: &str, tag: &str) {
    match nick_option {
        Some(nick) => search_engine.insert_tokens(id, &[nick, name, tag]),
        None => search_engine.insert_tokens(id, &[name, tag]),
    }
}

async fn add_guild_to_search_engine(ctx: &Context, guild_id: GuildId, user_search_map: &mut HashMap<u64, SimSearch<u64>>) {
    let search_options = SearchOptions::new().stop_words(vec!["#".to_string()]);
    let mut search_engine = SimSearch::new_with(search_options);
    let guild_adder = |guild: &Guild| {
        for (user_id, member) in &guild.members {
            let id = *user_id.as_u64();
            let nick = member.nick.as_deref();
            let name = member.user.name.as_str();
            let tag = member.user.tag();

            add_member_to_search_engine(nick, &mut search_engine, id, name, tag.as_str());
        }

        user_search_map.insert(guild.id.0, search_engine);
    };

    ctx.cache.guild_field(guild_id, guild_adder).await;
}

pub async fn on_self_join(ctx: &Context, guild_id: GuildId) {
    let mut data = ctx.data.write().await;
    let search_engine_map_option = data.get_mut::<UserSearchEngine>();

    if let Some(search_engine_map) = search_engine_map_option {
        add_guild_to_search_engine(ctx, guild_id, search_engine_map).await;
    }
}

pub async fn on_self_leave(ctx: &Context, guild_id: u64) {
    let mut data = ctx.data.write().await;
    let search_engine_map_option = data.get_mut::<UserSearchEngine>();

    if let Some(search_engine_map) = search_engine_map_option {
        search_engine_map.remove(&guild_id);
    }
}

pub async fn on_cache_ready(ctx: &Context) {
    let mut user_search_map = HashMap::with_capacity(ctx.cache.guild_count().await);

    for guild in ctx.cache.guilds().await {
        add_guild_to_search_engine(ctx, guild, &mut user_search_map).await;
    }

    let mut data = ctx.data.write().await;

    data.insert::<UserSearchEngine>(user_search_map);
}

pub async fn on_member_add(ctx: &Context, guild_id: u64, member: Member) {
    let mut data = ctx.data.write().await;

    if let Some(search_engine) = data.get_mut::<UserSearchEngine>().and_then(|engines| engines.get_mut(&guild_id)) {
        let id = member.user.id.0;
        let nick = member.nick.as_deref();
        let name = member.user.name.as_str();
        let tag = member.user.tag();

        add_member_to_search_engine(nick, search_engine, id, name, tag.as_str());
    }
}

pub async fn on_member_remove(ctx: &Context, guild_id: u64, user_id: u64) {
    let mut data = ctx.data.write().await;

    if let Some(search_engine) = data.get_mut::<UserSearchEngine>().and_then(|engines| engines.get_mut(&guild_id)) {
        search_engine.delete(&user_id);
    }
}

pub async fn user_id_search(ctx: &Context, guild_id: u64, user_str: &str) -> Option<Vec<u64>> {
    let data_read_lock = ctx.data.read().await;
    data_read_lock
        .get::<UserSearchEngine>()
        .and_then(|map| map.get(&guild_id))
        .map(|search_engine| search_engine.search(user_str))
}
