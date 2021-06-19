use std::collections::HashMap;

use serenity::client::Context;
use serenity::model::guild::Member;
use serenity::model::id::GuildId;
use serenity::prelude::TypeMapKey;
use simsearch::{SearchOptions, SimSearch};

pub struct UserSearchEngine;

impl TypeMapKey for UserSearchEngine {
    type Value = HashMap<u64, SimSearch<u64>>;
}

fn add_member_to_search_engine(nick_option: Option<String>, search_engine: &mut SimSearch<u64>, id: u64, name: &String, tag: &String) {
    if let Some(nick) = nick_option {
        search_engine.insert_tokens(id, &[nick.as_str(), name.as_str(), tag.as_str()]);
    } else {
        search_engine.insert_tokens(id, &[name.as_str(), tag.as_str()]);
    }
}

async fn add_guild_to_search_engine(ctx: &Context, guild_id: GuildId, user_search_map: &mut HashMap<u64, SimSearch<u64>>) {
    let search_options = SearchOptions::new().stop_words(vec!["#".to_string()]);
    let mut search_engine = SimSearch::new_with(search_options);
    let cache = ctx.cache.clone();
    let guild_option = guild_id.to_guild_cached(cache).await;

    if let Some(guild) = guild_option {
        for (user_id, member) in guild.members {
            let id = *user_id.as_u64();
            let user = member.user;
            let nick = member.nick;
            let name = &user.name;
            let tag = &user.tag();

            add_member_to_search_engine(nick, &mut search_engine, id, name, tag);
        }

        user_search_map.insert(guild.id.0, search_engine);
    }
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
    let cache = ctx.cache.clone();
    let mut user_search_map = HashMap::with_capacity(cache.guild_count().await);

    for guild in cache.guilds().await {
        add_guild_to_search_engine(ctx, guild, &mut user_search_map).await;
    }

    let mut data = ctx.data.write().await;

    data.insert::<UserSearchEngine>(user_search_map);
}

pub async fn on_member_add(ctx: &Context, guild_id: u64, member: Member) {
    let mut data = ctx.data.write().await;
    let search_engines_option = data.get_mut::<UserSearchEngine>();

    if let Some(search_engines) = search_engines_option {
        let search_engine_option = search_engines.get_mut(&guild_id);

        if let Some(search_engine) = search_engine_option {
            let nick = member.nick;
            let id = member.user.id.0;
            let name = &member.user.name;
            let tag = &member.user.tag();

            add_member_to_search_engine(nick, search_engine, id, name, tag);
        }
    }
}

pub async fn on_member_remove(ctx: &Context, guild_id: u64, user_id: u64) {
    let mut data = ctx.data.write().await;
    let search_engines_option = data.get_mut::<UserSearchEngine>();

    if let Some(search_engines) = search_engines_option {
        let search_engine_option = search_engines.get_mut(&guild_id);

        if let Some(search_engine) = search_engine_option {
            let id = &user_id;

            search_engine.delete(id);
        }
    }
}

pub async fn user_id_search(ctx: &Context, guild_id: u64, user_str: &str) -> Option<Vec<u64>> {
    let data = ctx.data.clone();
    let data_read_lock = data.read().await;
    let search_engine_option = data_read_lock.get::<UserSearchEngine>().and_then(|map| map.get(&guild_id));

    match search_engine_option {
        Some(search_engine) => Some(search_engine.search(user_str)),
        None => None,
    }
}
