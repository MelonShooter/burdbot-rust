use std::cmp;
use std::io::{Error, ErrorKind, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::{info, warn};
use serenity::client::Cache;
use serenity::http::Http;
use serenity::CacheAndHttp;
use tokio::runtime::Handle;
use tokio::task;

pub struct DiscordLogger {
    cache: Arc<Cache>,
    http: Arc<Http>,
    buffer_size: usize,
    failed_to_send_file: &'static str,
    write_cooldown: Duration,
    write_buffer: Arc<Mutex<Vec<u8>>>,
    message_buffer: Arc<Mutex<Vec<u8>>>,
    async_handle: Handle,
}

impl DiscordLogger {
    async fn send_logs(cache: Arc<Cache>, http: Arc<Http>, write_buffer: Arc<Mutex<Vec<u8>>>, message_buffer: Arc<Mutex<Vec<u8>>>) {
        // if file couldnt be sent, write a debug! message and output contents to an error file called "failed_to_send_file"
    }

    pub fn new<T: AsRef<CacheAndHttp>>(
        cache_and_http: T,
        buffer_size: usize,
        failed_to_send_file: &'static str,
        write_cooldown: Duration,
        async_handle: Handle,
    ) -> Self {
        let cache_and_http_ref = cache_and_http.as_ref();

        let logger = DiscordLogger {
            cache: cache_and_http_ref.cache.clone(),
            http: cache_and_http_ref.http.clone(),
            buffer_size,
            failed_to_send_file,
            write_cooldown,
            write_buffer: Arc::new(Mutex::new(Vec::with_capacity(buffer_size))),
            message_buffer: Arc::new(Mutex::new(Vec::with_capacity(buffer_size))),
            async_handle,
        };

        logger
    }
}

// Plan:
// make 2 internal buffers of the same size which are Arc<Mutex<Vec<u8>>>s within DiscordLogger //
// 1 buffer will be written to by the write functions and read from the task described below
// make sure to sanatize the write functions to only accept certain ASCII chars //
// make it so that new takes the size of the buffer to use (document the file size limit for discord) //
// and also takes an AsRef<CacheAndHttp>. it should then spawn a new tokio task
// which should sleep for 15 seconds in a loop and do the following after each sleep
// it must check that the private channel is ready
// do this by opening the private channel in the event handler on cache ready
// then within the tokio task loop, check if the private channel is in the cache
// if not, sleep again, if it is, continue running the rest of this
// it should read from the aforementioned buffer and copy it into a second one
// make sure the clear the first buffer after copying
// its important that after doing this, the lock to the first one is dropped
// read from the second buffer and make it into a text file, DMing it to me
// use the provided CacheAndHttp to send the embed (clone the Arc<Cache> and Arc<Http> and move them)
// after DMing it, clear the second buffer and loop back to sleep

impl Write for DiscordLogger {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // Sanatize writes to a restricted character set.
        if buf.iter().all(|b| b.is_ascii_graphic() || b.is_ascii_whitespace()) {
            let mut write_buffer = self.write_buffer.lock().map_or_else(|err| err.into_inner(), |b| b);
            let space_left = self.buffer_size - write_buffer.len();

            assert!(
                space_left > self.buffer_size,
                "space_left variable overflowed in DiscordLogger's write(), almost certainly because the buffer exceeded the allowed size: {}, which should never happen.",
                self.buffer_size
            );

            let bytes_to_write = cmp::min(space_left, buf.len());

            write_buffer.extend_from_slice(&buf[..bytes_to_write]);

            Ok(bytes_to_write)
        } else {
            let msg = format!("Non-graphic/whitespace ASCII was passed into DiscordLogger's write(). Investigate this. Bytes: {buf:?}.");

            warn!("{msg}");
            Err(Error::new(ErrorKind::InvalidData, msg))
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.async_handle.block_on(async {
            Self::send_logs(
                self.cache.clone(),
                self.http.clone(),
                self.write_buffer.clone(),
                self.message_buffer.clone(),
            )
            .await;
        });

        Ok(())
    }
}
