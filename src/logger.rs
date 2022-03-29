use std::io::Write;

pub struct DiscordLogger {}

impl DiscordLogger {
    pub fn new() -> Self {
        DiscordLogger {}
    }
}

impl Write for DiscordLogger {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        todo!()
    }

    fn flush(&mut self) -> std::io::Result<()> {
        todo!()
    }
}
