use std::collections::HashMap;
use std::ops::DerefMut;
use std::sync::Arc;
use std::time::Duration;

use log::error;
use serenity::client::{Cache, Context};
use serenity::http::Http;
use serenity::model::channel::{Channel, Message, PermissionOverwriteType};
use serenity::model::id::{ChannelId, RoleId};
use serenity::model::prelude::VoiceState;
use serenity::model::Permissions;
use serenity::prelude::{TypeMap, TypeMapKey};
use serenity::Error;
use tokio::task::JoinHandle;
use tokio::time;

use crate::commands;

const BOT_PREFIXES: [&str; 5] = ["-", "--", "---", "!", "!!"];
const ENGLISH_CLASS_CATEGORY_ID: u64 = 878362687837442098;
const MUSIC_CHANNEL_ID: u64 = 263643662808776704;
const ENGLISH_TEACHER_ROLE_ID: u64 = 878223433899577364;
const SPANISH_ENGLISH_SERVER_ID: u64 = 243838819743432704;
const ENGLISH_CLASS_STAGE_ID: u64 = 878363153455538246;

struct Teachers;

impl TypeMapKey for Teachers {
    type Value = HashMap<u64, Option<JoinHandle<()>>>;
}

async fn control_channel_access(http: &Http, channel: &Channel, allow: bool) -> Result<(), Error> {
    let everyone_id = RoleId::from(SPANISH_ENGLISH_SERVER_ID); // They're the same in this case.

    let mut permission_overwrite = match channel {
        Channel::Guild(ch) => ch
            .permission_overwrites
            .iter()
            .find(|p| p.kind == PermissionOverwriteType::Role(everyone_id))
            .cloned(),
        Channel::Category(cat) => cat
            .permission_overwrites
            .iter()
            .find(|p| p.kind == PermissionOverwriteType::Role(everyone_id))
            .cloned(),
        _ => None,
    }
    .expect("Every channel should have an @everyone permission overwrite");

    if allow {
        permission_overwrite.allow |= Permissions::READ_MESSAGES;
        permission_overwrite.deny &= !Permissions::READ_MESSAGES;
    } else {
        permission_overwrite.allow &= !Permissions::READ_MESSAGES;
    }

    match channel {
        Channel::Guild(ch) => ch.create_permission(http, &permission_overwrite).await,
        Channel::Category(cat) => cat.create_permission(http, &permission_overwrite).await,
        _ => Ok(()),
    }
}

async fn get_english_class_channels(cache: impl AsRef<Cache>) -> Vec<Channel> {
    let mut channels = Vec::new();

    let category = match cache.as_ref().channel(ENGLISH_CLASS_CATEGORY_ID).await {
        Some(cat) => cat,
        None => return channels,
    };

    channels.push(category);

    // Have to search channel map because of bug. Fixed in #1405 as a breaking change.
    match cache.as_ref().guild_channels(SPANISH_ENGLISH_SERVER_ID).await {
        Some(guild_channels) => {
            for (_, channel) in guild_channels {
                if channel.category_id.map(|c| c == ENGLISH_CLASS_CATEGORY_ID).unwrap_or(false) {
                    channels.push(Channel::Guild(channel));
                }
            }

            channels
        }
        None => channels,
    }
}

async fn get_teachers_present(ctx: &Context, english_channels: &[Channel]) -> Vec<u64> {
    let mut teachers = Vec::new();

    for ch in english_channels {
        if let Channel::Guild(channel) = ch {
            if !channel.is_text_based() {
                let members_roles: Option<Vec<(u64, Vec<RoleId>)>> = ctx
                    .cache
                    .guild_field(SPANISH_ENGLISH_SERVER_ID, |g| {
                        g.voice_states
                            .values()
                            .filter_map(|v| {
                                v.channel_id.and_then(|c| {
                                    if c == channel.id {
                                        let id = &v.user_id;

                                        g.members.get(id).map(|m| (id.0, m.roles.clone()))
                                    } else {
                                        None
                                    }
                                })
                            })
                            .collect()
                    })
                    .await;

                if let Some(members_roles) = members_roles {
                    let role_id = RoleId::from(ENGLISH_TEACHER_ROLE_ID);

                    for (id, roles) in members_roles {
                        if roles.contains(&role_id) {
                            teachers.push(id);
                        }
                    }
                }
            }
        }
    }

    teachers
}

async fn control_english_channel_access(http: Arc<Http>, english_channels: Vec<Channel>, allow: bool) {
    for channel in english_channels {
        let http = http.clone();

        tokio::spawn(async move {
            if let Err(error) = control_channel_access(&http, &channel, allow).await {
                error!("Error while setting channel access for English classes.{:?}", error);
            }
        });
    }
}

// TODO: make sure burdbot has access to channel afterwards.

async fn do_english_class_check(ctx: &Context, mut teacher_map: impl DerefMut<Target = TypeMap>) {
    let english_channels = get_english_class_channels(ctx).await;
    let teachers_present = &get_teachers_present(ctx, &english_channels).await;
    let teacher_map = teacher_map
        .deref_mut()
        .get_mut::<Teachers>()
        .expect("Teachers should always exist in the type map.");

    for teacher in teachers_present {
        teacher_map.insert(*teacher, None);
    }

    control_english_channel_access(ctx.http.clone(), english_channels, !teachers_present.is_empty()).await;
}

pub async fn on_voice_state_update(old_state: Option<&VoiceState>, new_state: &VoiceState, ctx: &Context) {
    // Someone left the stage channel
    if old_state.map_or(false, |v| v.channel_id == Some(ChannelId::from(ENGLISH_CLASS_STAGE_ID))) {
        let teacher_id = new_state.user_id.0;
        let mut write_data = ctx.data.write().await;
        let is_teacher_leaving = {
            let teachers = match write_data.get::<Teachers>() {
                Some(teachers) => teachers,
                None => return,
            };

            teachers.contains_key(&teacher_id)
        };

        let cache = ctx.cache.clone();
        let http = ctx.http.clone();
        let data = ctx.data.clone();

        if is_teacher_leaving {
            let teachers = write_data
                .get_mut::<Teachers>()
                .expect("Teachers should be a thing due to the match above.");

            if teachers.len() > 1 {
                teachers.remove(&teacher_id);
            } else {
                let teacher_task = teachers
                    .get_mut(&teacher_id)
                    .expect("The teachers should always exist due to the match above and the lock.");
                *teacher_task = Some(tokio::spawn(async move {
                    time::sleep(Duration::from_secs(60 * 10)).await;

                    let mut write_data = data.write().await;

                    if let Some(teachers) = write_data.get_mut::<Teachers>() {
                        control_english_channel_access(http, get_english_class_channels(cache).await, false).await;
                        teachers.remove(&teacher_id);
                    };
                }));
            }
        }
    } else if new_state.channel_id == Some(ChannelId::from(ENGLISH_CLASS_STAGE_ID)) {
        let mut write_data = ctx.data.write().await;
        // Someone joined the stage channel.
        let teacher_id = match &new_state.member {
            Some(m) => {
                if m.roles.contains(&RoleId::from(ENGLISH_TEACHER_ROLE_ID)) {
                    Some(m.user.id)
                } else {
                    None
                }
            }
            None => None,
        };

        if let Some(id) = teacher_id {
            let teachers = match write_data.get_mut::<Teachers>() {
                Some(teachers) => teachers,
                None => return,
            };

            let join_handle_to_cancel = teachers.insert(id.0, None);

            if let Some(join_handle) = join_handle_to_cancel.flatten() {
                join_handle.abort();
            }

            control_english_channel_access(ctx.http.clone(), get_english_class_channels(ctx).await, true).await;
        }
    }
}

pub async fn on_cache_ready(ctx: &Context) {
    // TODO: on cache ready, make sure to create and update the Teachers data
    let data = &ctx.data;
    let mut write_data = data.write().await;

    write_data.insert::<Teachers>(HashMap::new());

    do_english_class_check(ctx, write_data).await;
}

pub async fn on_message_receive(ctx: &Context, message: &Message) {
    do_music_check(ctx, message).await;
}

async fn do_music_check(ctx: &Context, message: &Message) {
    let channel_id = message.channel_id.0;

    if channel_id != MUSIC_CHANNEL_ID {
        return;
    }

    let content = message.content.as_str();

    for prefix in BOT_PREFIXES {
        if content.starts_with(prefix) {
            let msg_str = "Please put music bot commands in <#247135634265735168> as they do not work here. \
            Por favor, poné los comandos de música en <#247135634265735168>. No funcionan por acá.";

            commands::send_message(ctx, &message.channel_id, msg_str, "on_message_receive").await;

            return;
        }
    }
}
