//! DESIGN:
//! To add an image:
//!      - User can provide a description of the images and a message link that contains images to add to filter
//!      - Then, the ImageChecker generates a hash for each image and puts all the info into the DB
//!      - Throw error if linked message has more than 1 image
//! To get a list of banned images:
//!      - Users can run a command to query all banned images
//!      - This can be expanded later to filter by dimensions or hash (index is already in place for this)
//! To remove an image:
//!      - User removes based on original message link given for the image
//!
//! Internally:
//! - Checking for an image:
//!     - First we get the width and height. If there's a match, then we download the image and check
//!       its hash to see if it matches against the images retrieved.
//!
//! Represents all the images in a message, including
//! attachments, image embeds, and thumbnail embeds

use std::{io, marker::PhantomData};

use chrono::Utc;
use digest::{Digest, Output};
use log::info;
use reqwest::Client;
use rusqlite::{Connection, Row, params};
use serenity::all::{GuildId, Message};
use strum_macros::Display;

use crate::{BURDBOT_DB, error::SerenitySQLiteResult};

/// Sets the byte limit until the image hash becomes a blocking task.
/// Currently 9MB
const LIMIT_FOR_BLOCKING_TASK: usize = 9_000_000;

pub struct MessageImages<'a>(pub &'a Message);

impl<'a> MessageImages<'a> {
    pub fn to_vec(&self) -> Vec<(&'_ str, u32, u32)> {
        if self.0.attachments.len() + self.0.embeds.len() == 0 {
            return Vec::new();
        }

        let attach_images =
            self.0.attachments.iter().flat_map(|a| Some((a.url.as_str(), a.width?, a.height?)));
        let embed_images = self
            .0
            .embeds
            .iter()
            .flat_map(|e| e.image.as_ref().map(|e| Some((e.url.as_str(), e.width?, e.height?))))
            .flatten();
        let embed_thumbnails = self
            .0
            .embeds
            .iter()
            .flat_map(|e| e.thumbnail.as_ref().map(|e| Some((e.url.as_str(), e.width?, e.height?))))
            .flatten();

        attach_images.chain(embed_images).chain(embed_thumbnails).collect::<Vec<_>>()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ImageChecker<T: Digest>(PhantomData<T>);

#[derive(Debug, Clone)]
pub struct ImageResult {
    pub link_ref: String,
    pub width: u32,
    pub height: u32,
    pub description: String,
    pub hash_hex: String,
    pub hash_type: u32,
}

impl TryFrom<&Row<'_>> for ImageResult {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        let link_ref = row.get(0)?;
        let width = row.get(1)?;
        let height = row.get(2)?;
        let description = row.get(3)?;
        let hash_hex = hex::encode(row.get::<_, Vec<u8>>(4)?);
        let hash_type = row.get(5)?;

        Ok(ImageResult { link_ref, width, height, description, hash_hex, hash_type })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Display)]
#[non_exhaustive]
pub enum ImageOpOutcome {
    #[strum(to_string = "Operation succeeded")]
    Success,
    #[strum(to_string = "Must provide message with exactly one image")]
    NotOneAttachment,
    #[strum(to_string = "Message provided has no banned image")]
    NotFound,
    #[strum(to_string = "Image from message provided is already banned")]
    Duplicate,
}

impl<T: Digest> ImageChecker<T> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }

    // Calculates the hash of the given image
    async fn calc_image_hash(&self, url: &str) -> serenity::Result<Output<T>> {
        let reqwest = Client::new();
        let bytes = reqwest.get(url).send().await?.bytes().await?;
        let len = bytes.len();
        let task = move || T::new().chain_update(bytes).finalize();

        if len >= LIMIT_FOR_BLOCKING_TASK {
            let now = Utc::now();

            let task_result = tokio::task::spawn_blocking(task)
                .await
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e));

            info!(
                "Large file received. {len} bytes of data. Hash in blocking thread took {}ms...",
                (Utc::now() - now).num_microseconds().unwrap_or(-1)
            );

            Ok(task_result?)
        } else {
            Ok(task())
        }
    }

    // Adds an image to the image checker for the guild
    // Returns Success if successful, otherwise if:
    // - image couldn't be added b/c message had no image or more than 1 (NotOneAttachment)
    // - the entry already exists in the checker (Duplicate)
    // An Err indicates some internal error occurred.
    pub async fn add_image(
        &self, desc: &str, guild_id: GuildId, message: &Message, hash_type: impl Into<u16>,
    ) -> SerenitySQLiteResult<ImageOpOutcome> {
        let message_images = MessageImages(message);
        let images = message_images.to_vec();

        if images.len() != 1 {
            return Ok(ImageOpOutcome::NotOneAttachment);
        }

        let (url, width, height) = images[0];
        let hash = self.calc_image_hash(url).await?;
        let link = message.id.link(message.channel_id, Some(guild_id));
        let connection = Connection::open(BURDBOT_DB)?;
        let insertion_statement = "
                INSERT OR IGNORE INTO fxhash_image_checksums
                    VALUES (?, ?, ?, ?, ?, ?, ?);
        ";

        let rows_updated = connection.execute(
            insertion_statement,
            params!(link, width, height, desc, &hash[..], hash_type.into(), guild_id.get()),
        )?;

        // If no rows updated, there was a duplicate
        Ok(if rows_updated == 0 { ImageOpOutcome::Duplicate } else { ImageOpOutcome::Success })
    }

    // Removes an image from the image checker for the guild.
    // Returns NotFound if not found in guild, otherwise Success, unless an internal error occurs
    pub fn remove_image(
        &self, guild_id: GuildId, msg_link: &str,
    ) -> SerenitySQLiteResult<ImageOpOutcome> {
        let connection = Connection::open(BURDBOT_DB)?;
        let deletion_statement = "
                DELETE FROM fxhash_image_checksums
                    WHERE link_reference = ? AND guild_id = ?;
        ";

        let rows_updated =
            connection.execute(deletion_statement, params!(msg_link, guild_id.get()))?;

        // If no rows updated, then it wasn't in the checker
        Ok(if rows_updated == 0 { ImageOpOutcome::NotFound } else { ImageOpOutcome::Success })
    }

    // Checks if an image passes the filters for the guild.
    // Returns true if no image was found in the checker. Err if there was an internal error
    pub async fn check_image(
        &self, guild_id: GuildId, image: (&str, u32, u32),
    ) -> SerenitySQLiteResult<bool> {
        let (url, width, height) = image;
        let rows;

        info!("Got attachments {image:?}");

        {
            let connection = Connection::open(BURDBOT_DB)?;
            let mut hash_query = connection.prepare(
                "
                SELECT hash FROM fxhash_image_checksums
                WHERE guild_id = ?1 AND width = ?2 AND height = ?3;
                ",
            )?;

            rows = hash_query
                .query_and_then(params![guild_id.get(), width, height], |row| {
                    row.get::<_, Vec<u8>>(0)
                })?
                .collect::<rusqlite::Result<Vec<Vec<u8>>>>()?;
        }

        // Means no images with matching dimension found
        if rows.is_empty() {
            return Ok(true);
        }

        // Now check the checksum.
        let attachment_hash = self.calc_image_hash(url).await?;

        for hash in rows {
            if &hash[..] == &attachment_hash[..] {
                return Ok(false);
            }
        }

        Ok(true)
    }

    // Gets the images stored for a guild
    pub fn get_images(&self, guild_id: GuildId) -> SerenitySQLiteResult<Vec<ImageResult>> {
        // TODO: expand later to filter by width and height, or by link

        let connection = Connection::open(BURDBOT_DB)?;
        let mut image_query = connection.prepare_cached(
            "
                SELECT link_reference, width, height, description, hash, hash_type FROM fxhash_image_checksums
                WHERE guild_id = ?1;
        ",
        )?;

        // query_and_then returns a vector of rusqlite::Result<ImageResult>, so collect into one
        let result = image_query
            .query_and_then(params![guild_id.get()], |row| ImageResult::try_from(row))?
            .collect::<rusqlite::Result<Vec<ImageResult>>>()?;

        Ok(result)
    }
}
