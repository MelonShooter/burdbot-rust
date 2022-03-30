use std::io::{Error, ErrorKind, Result, Write};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use std::{cmp, iter};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex as TokioMutex;

use log::{error, info, warn};
use once_cell::sync::OnceCell;
use serenity::client::Context;
use serenity::http::CacheHttp;
use serenity::model::id::{ChannelId, UserId};
use serenity::CacheAndHttp;
use std::str;
use tokio::runtime::Handle;
use tokio::time;

use crate::DELIBURD_ID;

struct LogSender {
    cache_and_http: Arc<CacheAndHttp>,
    failed_to_send_file: &'static str,
    send_file_name: &'static str,
    write_buffer: Arc<StdMutex<Vec<u8>>>,
    message_buffer: Arc<TokioMutex<Vec<u8>>>,
}

static DELIBURD_CHANNEL_ID: OnceCell<Option<ChannelId>> = OnceCell::new();

async fn get_deliburd_channel_id(cache_and_http: impl CacheHttp) -> Option<ChannelId> {
    let channel = UserId::from(DELIBURD_ID).create_dm_channel(cache_and_http).await;

    match channel {
        Ok(channel) => Some(channel.id),
        Err(err) => {
            error!("Couldn't create DM channel with {DELIBURD_ID} to send logs to. Error: {err}\nSending logs to fallback file instead.");

            None
        }
    }
}

pub async fn on_cache_ready(ctx: &Context) {
    let set_result = DELIBURD_CHANNEL_ID.set(get_deliburd_channel_id(ctx).await);

    if let Err(_) = set_result {
        info!("The DM channel ID OnceCell already had a value in it. This can only happen if the cache wasn't ready fast enough.");
    }
}

impl LogSender {
    async fn send_to_file(&self) -> Result<()> {
        let mut file = File::create(self.failed_to_send_file).await?;
        let message_buffer = self.message_buffer.lock().await;

        file.write_all(message_buffer.as_slice()).await?;

        Ok(())
    }

    async fn send(&self) {
        let channel_id_option = match DELIBURD_CHANNEL_ID.get() {
            Some(option) => option,
            None => DELIBURD_CHANNEL_ID
                .try_insert(get_deliburd_channel_id(&self.cache_and_http).await)
                .unwrap_or_else(|(option, _)| option),
        };

        if let &Some(id) = channel_id_option {
            let mut message_buffer = self.message_buffer.lock().await;

            {
                let mut write_buffer = self.write_buffer.lock().unwrap_or_else(|err| err.into_inner());

                message_buffer.clear();
                message_buffer.append(&mut write_buffer);
            }

            let files = iter::once((message_buffer.as_slice(), self.send_file_name));

            if let Err(err) = id.send_files(&self.cache_and_http.http, files, |m| m).await {
                eprintln!(
                    "Failed to send log message. Encountered Serenity error: {err}\nSending logs to fallback file '{}' instead.",
                    self.failed_to_send_file
                )
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
            cache_and_http: logger.cache_http.clone(),
            failed_to_send_file: logger.failed_to_send_file,
            send_file_name: logger.send_file_name,
            write_buffer: logger.write_buffer.clone(),
            message_buffer: logger.message_buffer.clone(),
        }
    }
}

pub struct DiscordLogger {
    cache_http: Arc<CacheAndHttp>,
    buffer_size: usize,
    failed_to_send_file: &'static str,
    send_file_name: &'static str,
    write_buffer: Arc<StdMutex<Vec<u8>>>,
    message_buffer: Arc<TokioMutex<Vec<u8>>>,
    async_handle: Handle,
}

impl DiscordLogger {
    pub fn new(
        cache_and_http: Arc<CacheAndHttp>,
        buffer_size: usize,
        failed_to_send_file: &'static str,
        send_file_name: &'static str,
        write_cooldown: Duration,
        async_handle: Handle,
    ) -> Self {
        let logger = DiscordLogger {
            cache_http: cache_and_http.clone(),
            buffer_size,
            failed_to_send_file,
            send_file_name,
            write_buffer: Arc::new(StdMutex::new(Vec::with_capacity(buffer_size))),
            message_buffer: Arc::new(TokioMutex::new(Vec::with_capacity(buffer_size))),
            async_handle,
        };
        let log_sender = LogSender::from(&logger);

        tokio::spawn(async move {
            loop {
                time::sleep(write_cooldown).await;

                log_sender.send().await;
            }
        });

        logger
    }
}

fn malformed_string_err(buf: &[u8]) -> Error {
    let msg = format!("Non-graphic/whitespace ASCII was passed into DiscordLogger's write(). Investigate this. Bytes: {buf:?}.");

    warn!("{msg}");

    Error::new(ErrorKind::InvalidData, msg)
}

impl Write for DiscordLogger {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        // Sanitize writes and show in stderr.
        match str::from_utf8(buf) {
            Ok(str) => eprintln!("{}", str),
            Err(_) => return Err(malformed_string_err(buf)),
        };

        let mut write_buffer = self.write_buffer.lock().unwrap_or_else(|err| err.into_inner());
        let space_left = self.buffer_size - write_buffer.len();

        assert!(
                space_left > self.buffer_size,
                "space_left variable overflowed in DiscordLogger's write(), almost certainly because the buffer exceeded the allowed size: {}, which should never happen.",
                self.buffer_size
            );

        let bytes_to_write = cmp::min(space_left, buf.len());

        write_buffer.extend_from_slice(&buf[..bytes_to_write]);

        Ok(bytes_to_write)
    }

    fn flush(&mut self) -> Result<()> {
        self.async_handle.block_on(async {
            LogSender::from(&*self).send().await;
        });

        Ok(())
    }
}
