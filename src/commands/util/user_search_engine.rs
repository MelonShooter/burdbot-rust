use std::collections::HashMap;
use std::time::Instant;

use serenity::client::Context;
use serenity::prelude::TypeMapKey;
use simsearch::{SearchOptions, SimSearch};

pub struct UserSearchEngine;

impl TypeMapKey for UserSearchEngine {
    type Value = HashMap<u64, SimSearch<u64>>;
}

pub async fn on_cache_ready(ctx: Context) {
    let cache = ctx.cache.clone();
    let mut user_search_map = HashMap::with_capacity(cache.guild_count().await);

    for guild in ctx.cache.guilds().await {
        let search_options = SearchOptions::new().stop_words(vec!("#".to_string()));
        let mut search_engine = SimSearch::new_with(search_options);
        let cache = ctx.cache.clone();
        let guild_option = guild.to_guild_cached(cache).await;

        if let Some(guild) = guild_option {
            for (user_id, member) in guild.members {
                let id = *user_id.as_u64();
                let user = member.user;
                let name = &user.name;
                let tag = user.tag();

                if let Some(nick) = member.nick {
                    search_engine.insert_tokens(id, &[nick.as_str(), name.as_str(), tag.as_str()]);
                } else {
                    search_engine.insert_tokens(id, &[name.as_str(), tag.as_str()]);
                }
            }
        }

        user_search_map.insert(*guild.as_u64(), search_engine);
    }

    let search_engine = user_search_map.get(&243838819743432704).unwrap();

    let instant = Instant::now();
    println!("{:?}", search_engine.search("delibrd"));
    println!("{}", (Instant::now() - instant).as_millis());
    let instant = Instant::now();
    println!("{:?}", search_engine.search("laurens"));
    println!("{}", (Instant::now() - instant).as_millis());
    let instant = Instant::now();
    println!("{:?}", search_engine.search("nicl"));
    println!("{}", (Instant::now() - instant).as_millis());
    let mut data = ctx.data.write().await;
    
    data.insert::<UserSearchEngine>(user_search_map);
}