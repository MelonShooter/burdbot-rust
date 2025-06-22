use std::marker::PhantomData;

use digest::{Digest, Output};
use rusqlite::{Connection, Row, params};
use serenity::all::{Attachment, GuildId, Message};

use crate::{BURDBOT_DB, error::SerenitySQLiteResult};

// DESIGN:
// To add an image:
//      - User can provide a description of the images and a message link that contains attachments to add to filter
//      - Then, the ImageChecker generates a hash for each attachment and puts all the info into the DB
//      - Throw error if linked message has more than 1 image
// To get a list of banned images:
//      - Users can run a command to query all banned images
//      - This can be expanded later to filter by dimensions or hash (index is already in place for this)
// To remove an image:
//      - User removes based on original message link given for the attachment
//
// Internally:
// - Checking for an image:
//     - First we get the width and height. If there's a match, then we download the image and check
//       its hash to see if it matches against the images retrieved.

#[derive(Debug, Copy, Clone)]
pub struct ImageChecker<T: Digest>(PhantomData<T>);

// link_reference TEXT PRIMARY KEY,
// width INTEGER NOT NULL,
// height INTEGER NOT NULL,
// description TEXT NOT NULL,
// hash BLOB NOT NULL,
// hash_type INTEGER NOT NULL,
// guild_id INTEGER NOT NULL

#[derive(Debug, Clone)]
pub struct ImageResult {
    link_ref: String,
    width: u32,
    height: u32,
    description: String,
    hash_hex: String,
    hash_type: u32,
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageOpOutcome {
    Success,
    NotOneAttachment,
    NotImage,
    NotFromGuild,
    Duplicate,
}

impl<T: Digest> ImageChecker<T> {
    // Calculates the hash of the given attachment
    async fn calc_attachment_hash(
        &self,
        attachment: &Attachment,
    ) -> SerenitySQLiteResult<Output<T>> {
        let bytes = attachment.download().await?;
        Ok(T::new().chain_update(bytes).finalize())
    }

    // Adds an image to the image checker for the guild
    // Returns Success if successful, otherwise if:
    // - image couldn't be added b/c message had no attachments or more than 1 (NotOneAttachment)
    // - the provided attachment wasn't an image (NotImage)
    // - the entry already exists in the checker (Duplicate)
    // - the message wasn't from a guild (NotFromGuild)
    // An Err indicates some internal error occurred.
    pub async fn add_image(
        &self,
        desc: &str,
        message: &Message,
        hash_type: impl Into<u16>,
    ) -> SerenitySQLiteResult<ImageOpOutcome> {
        if message.attachments.len() > 1 || message.attachments.is_empty() {
            return Ok(ImageOpOutcome::NotOneAttachment);
        }

        let attachment = &message.attachments[0];
        let Some((width, height)) = attachment.dimensions() else {
            return Ok(ImageOpOutcome::NotImage);
        }; // This checks that the attachment is an image
        let hash = self.calc_attachment_hash(attachment).await?;
        let link = message.link();
        let Some(guild_id) = message.guild_id else {
            return Ok(ImageOpOutcome::NotFromGuild);
        };

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
    // Returns NotImage if not found in guild, otherwise Success, unless an internal error occurs
    pub fn remove_image(guild_id: GuildId, msg_link: &str) -> SerenitySQLiteResult<ImageOpOutcome> {
        let connection = Connection::open(BURDBOT_DB)?;
        let deletion_statement = "
                DELETE FROM fxhash_image_checksums
                    WHERE link_reference = ? AND guild_id = ?;
        ";

        let rows_updated =
            connection.execute(deletion_statement, params!(msg_link, guild_id.get()))?;

        // If no rows updated, then it wasn't in the checker
        Ok(if rows_updated == 0 { ImageOpOutcome::NotImage } else { ImageOpOutcome::Success })
    }

    // Checks if an image passes the filters for the guild.
    // Returns true if no image was found in the checker. Err if there was an internal error
    pub async fn check_image(
        guild_id: GuildId,
        attachment: Attachment,
    ) -> SerenitySQLiteResult<bool> {
        let Some((width, height)) = attachment.dimensions() else {
            return Ok(false);
        };

        let connection = Connection::open(BURDBOT_DB)?;
        let mut checksum_query = connection.prepare_cached(
            "
                SELECT checksum FROM bday_role_list
                WHERE guild_id = ?1 AND width = ?2 AND height = ?3;
        ",
        )?;

        let rows = checksum_query
            .query_and_then(params![guild_id.get(), width, height], |row| {
                row.get::<_, Vec<u8>>(0)
            })?;

        let mut download = Vec::new();

        // Lazily download attachment if a row has been found
        for row in rows {
            let row = row?;

            if download.is_empty() {
                download = attachment.download().await?;
            }

            if row == download {
                return Ok(false);
            }
        }

        Ok(true)
    }

    // Gets the images stored for a guild
    pub fn get_images(guild_id: GuildId) -> SerenitySQLiteResult<Vec<ImageResult>> {
        // TODO: expand later to filter by width and height, or by link

        let connection = Connection::open(BURDBOT_DB)?;
        let mut image_query = connection.prepare_cached(
            "
                SELECT link_reference, width, height, description, hash, hash_type FROM bday_role_list
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
