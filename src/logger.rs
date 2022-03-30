use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serenity::client::Cache;
use serenity::http::Http;
use serenity::CacheAndHttp;

// Plan:
// make 2 internal buffers of the same size which are Arc<Mutex<Vec<u8>>>s within DiscordLogger
// 1 buffer will be written to by the write functions and read from the task described below
// make sure to sanatize the write functions to only accept certain ASCII chars
// make it so that new takes the size of the buffer to use (document the file size limit for discord)
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

pub struct DiscordLogger {
    cache: Arc<Cache>,
    http: Arc<Http>,
    buffer_size: usize,
    write_cooldown: Duration,
    write_buffer: Arc<Mutex<Vec<u8>>>,
    message_buffer: Arc<Vec<u8>>, // check whether get_mut can just be used here or if it needs to be a mutex, make sure cloning wouldnt cause issues here
}

impl DiscordLogger {
    pub fn new<T: AsRef<CacheAndHttp>>(cache_and_http: T, buffer_size: usize, write_cooldown: Duration) -> Self {
        let cache_and_http_ref = cache_and_http.as_ref();

        DiscordLogger {
            cache: cache_and_http_ref.cache.clone(),
            http: cache_and_http_ref.http.clone(),
            buffer_size,
            write_cooldown,
            write_buffer: Arc::new(Mutex::new(Vec::with_capacity(buffer_size))),
            message_buffer: Arc::new(Vec::with_capacity(buffer_size)),
        }
    }
}

impl Write for DiscordLogger {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        eprint!("{}", String::from_utf8_lossy(buf));
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
