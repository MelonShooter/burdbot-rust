use serenity::all::{Cache, CreateAttachment, CreateMessage, Http};
use std::fmt::Debug;
use std::io::{Error, ErrorKind, Write};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use std::{cmp, iter};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::mpsc::UnboundedSender;

use log::{info, warn};
use once_cell::sync::OnceCell;
use serenity::client::Context;
use serenity::http::CacheHttp;
use serenity::model::id::{ChannelId, UserId};
use std::str;
use tokio::time;

use crate::DELIBURD_ID;

pub struct LogSender {
    cache: Arc<Cache>,
    http: Arc<Http>,
    failed_to_send_file: &'static str,
    send_file_name: &'static str,
    write_buffer: Arc<StdMutex<Vec<u8>>>,
    message_buffer: Arc<TokioMutex<Vec<u8>>>,
}

impl Debug for LogSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogSender")
            .field("cache", &"omitted")
            .field("http", &"omitted")
            .field("failed_to_send_file", &self.failed_to_send_file)
            .field("send_file_name", &self.send_file_name)
            .field("write_buffer", &self.write_buffer)
            .field("message_buffer", &self.message_buffer)
            .finish()
    }
}

static DELIBURD_CHANNEL_ID: OnceCell<Option<ChannelId>> = OnceCell::new();

async fn get_deliburd_channel_id(cache_and_http: impl CacheHttp) -> Option<ChannelId> {
    let channel = UserId::from(DELIBURD_ID).create_dm_channel(cache_and_http).await;

    match channel {
        Ok(channel) => Some(channel.id),
        Err(err) => {
            eprintln!(
                "Couldn't create DM channel with {DELIBURD_ID} to send logs to. Error: {err}\nSending logs to fallback file instead."
            );

            None
        },
    }
}

pub(crate) async fn on_cache_ready(ctx: &Context) {
    let set_result = DELIBURD_CHANNEL_ID.set(get_deliburd_channel_id(ctx).await);

    if set_result.is_err() {
        info!(
            "The DM channel ID OnceCell already had a value in it. This can only happen if the cache wasn't ready fast enough."
        );
    }
}

impl LogSender {
    async fn send_to_file(&self) -> std::io::Result<()> {
        let mut file = File::create(self.failed_to_send_file).await?;
        let message_buffer = self.message_buffer.lock().await;

        file.write_all(message_buffer.as_slice()).await?;

        Ok(())
    }

    pub async fn send(&self) {
        let channel_id_option = match DELIBURD_CHANNEL_ID.get() {
            Some(option) => option,
            None => DELIBURD_CHANNEL_ID
                .try_insert(get_deliburd_channel_id((&self.cache, &*self.http)).await)
                .unwrap_or_else(|(option, _)| option),
        };

        if let &Some(id) = channel_id_option {
            let mut message_buffer = self.message_buffer.lock().await;

            {
                let mut write_buffer =
                    self.write_buffer.lock().unwrap_or_else(|err| err.into_inner());

                if write_buffer.is_empty() {
                    return;
                }

                message_buffer.clear();
                message_buffer.append(&mut write_buffer);
            }

            let files =
                iter::once(CreateAttachment::bytes(message_buffer.as_slice(), self.send_file_name));

            match id.send_files((&self.cache, &*self.http), files, CreateMessage::new()).await {
                Err(err) => eprintln!(
                    "Failed to send log message. Encountered Serenity error: {err}\nSending logs to fallback file '{}' instead.",
                    self.failed_to_send_file
                ),
                _ => return,
            }
        }

        if let Err(err) = self.send_to_file().await {
            eprintln!("Failed to write log to fallback file. Encountered IO error: {err}");
        }
    }
}

impl From<&DiscordLogger> for LogSender {
    fn from(logger: &DiscordLogger) -> Self {
        LogSender {
            cache: logger.cache.clone(),
            http: logger.http.clone(),
            failed_to_send_file: logger.failed_to_send_file,
            send_file_name: logger.send_file_name,
            write_buffer: logger.write_buffer.clone(),
            message_buffer: logger.message_buffer.clone(),
        }
    }
}

pub struct DiscordLogger {
    cache: Arc<Cache>,
    http: Arc<Http>,
    buffer_size: usize,
    failed_to_send_file: &'static str,
    send_file_name: &'static str,
    write_buffer: Arc<StdMutex<Vec<u8>>>,
    message_buffer: Arc<TokioMutex<Vec<u8>>>,
    sender: UnboundedSender<LogSender>,
}

impl DiscordLogger {
    pub fn new(
        cache: Arc<Cache>, http: Arc<Http>, buffer_size: usize, failed_to_send_file: &'static str,
        send_file_name: &'static str, write_cooldown: Duration, sender: UnboundedSender<LogSender>,
    ) -> Self {
        let logger = DiscordLogger {
            cache,
            http,
            buffer_size,
            failed_to_send_file,
            send_file_name,
            write_buffer: Arc::new(StdMutex::new(Vec::with_capacity(buffer_size))),
            message_buffer: Arc::new(TokioMutex::new(Vec::with_capacity(buffer_size))),
            sender,
        };
        let log_sender = LogSender::from(&logger);

        tokio::spawn(async move {
            loop {
                time::sleep(write_cooldown).await;

                if DELIBURD_CHANNEL_ID.get().is_none() {
                    continue; // Sleep until there's something in OnceCell.
                }

                log_sender.send().await;
            }
        });

        logger
    }
}

fn malformed_string_err(buf: &[u8]) -> std::io::Error {
    let msg = format!(
        "Non-UTF-8 bytes were passed into DiscordLogger's write(). Investigate this. Bytes: {buf:?}."
    );

    warn!("{msg}");

    Error::new(ErrorKind::InvalidData, msg)
}

impl Write for DiscordLogger {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // does rejecting a write cause the thread to panic? like an Ok of size 0?
        // Sanitize writes and show in stderr.
        match str::from_utf8(buf) {
            Ok(str) => eprint!("{str}"),
            Err(_) => return Err(malformed_string_err(buf)),
        };

        let mut write_buffer = self.write_buffer.lock().unwrap_or_else(|err| err.into_inner());
        let space_left = self.buffer_size - write_buffer.len();

        assert!(
            space_left <= self.buffer_size,
            "space_left variable overflowed in DiscordLogger's write(), almost certainly because the buffer exceeded the allowed size: {}, which should never happen.",
            self.buffer_size
        );

        let bytes_to_write = cmp::min(space_left, buf.len());

        write_buffer.extend_from_slice(&buf[..bytes_to_write]);

        Ok(bytes_to_write)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.sender.send(LogSender::from(&*self)).map_err(Error::other)
    }
}
