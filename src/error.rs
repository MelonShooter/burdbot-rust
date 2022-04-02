use std::fmt::Debug;
use thiserror::Error;

pub type SerenitySQLiteResult<T> = std::result::Result<T, SerenitySQLiteError>;

#[derive(Debug, Error)]
#[error("{serenity_errors:?}")]
pub struct SerenityErrors {
    pub serenity_errors: Vec<serenity::Error>,
}

impl SerenityErrors {
    pub fn new(serenity_errors: Vec<serenity::Error>) -> Self {
        SerenityErrors { serenity_errors }
    }
}

impl From<serenity::Error> for SerenityErrors {
    fn from(error: serenity::Error) -> Self {
        Self::new(vec![error])
    }
}

impl From<Vec<serenity::Error>> for SerenityErrors {
    fn from(errors: Vec<serenity::Error>) -> Self {
        Self::new(errors)
    }
}

#[derive(Error, Debug)]
pub enum SerenitySQLiteError {
    #[error("Serenity errors encountered: {0:?}")]
    SerenityError(#[from] SerenityErrors),
    #[error("SQLite error encountered: {0:?}")]
    SQLiteError(#[from] rusqlite::Error),
}

impl From<serenity::Error> for SerenitySQLiteError {
    fn from(errors: serenity::Error) -> Self {
        errors.into()
    }
}

impl From<Vec<serenity::Error>> for SerenitySQLiteError {
    fn from(errors: Vec<serenity::Error>) -> Self {
        errors.into()
    }
}
