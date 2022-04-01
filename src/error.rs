use rusqlite::Error as SQLiteError;
use serenity::Error as SerenityError;
use std::fmt::Debug;
use strum::EnumProperty;
use thiserror::Error;

use crate::commands::ConversionType;

#[derive(Error, Debug)]
pub enum ArgumentParseError {
    #[error("{0}")]
    OutOfBounds(#[from] ArgumentOutOfBoundsError),
    #[error("{0}")]
    NotEnoughArguments(#[from] NotEnoughArgumentsError),
    #[error("{0}")]
    ArgumentConversionError(#[from] ArgumentConversionError),
    #[error("{0}")]
    BadOption(#[from] BadOptionError),
}

#[derive(Error, Debug, Clone)]
#[error("Invalid choice in argument #{arg_pos}. Choices are {choices}. The argument provided was {provided_choice}")]
pub struct BadOptionError {
    pub arg_pos: usize,
    pub provided_choice: String,
    pub choices: String,
}

impl BadOptionError {
    pub fn new(arg_pos: usize, provided_choice: String, choices: String) -> Self {
        Self {
            arg_pos,
            provided_choice,
            choices,
        }
    }
}

#[derive(Error, Debug, Copy, Clone)]
#[error("Not enough arguments provided. At least {min_args} arg(s) is/are needed. {args_provided} was/were provided.")]
pub struct NotEnoughArgumentsError {
    pub min_args: usize,
    pub args_provided: usize,
}

impl NotEnoughArgumentsError {
    pub fn new(min_args: usize, args_provided: usize) -> Self {
        Self { min_args, args_provided }
    }
}

#[derive(Error, Debug, Copy, Clone)]
#[error("Argument #{arg_pos} is out of bounds. The range (inclusive) for this argument is {lower} to {upper}. The number provided was {arg}.")]
pub struct ArgumentOutOfBoundsError {
    pub lower: i64,
    pub upper: i64,
    pub arg: i64,
    pub arg_pos: usize,
}

impl ArgumentOutOfBoundsError {
    pub fn new(lower: i64, upper: i64, arg: i64, arg_pos: usize) -> Self {
        Self { lower, upper, arg, arg_pos }
    }
}

const CONVERSION_NO_INFO: &str = "Conversions should always have an info property";

#[derive(Error, Debug, Clone)]
#[error("Argument #{arg_pos} could not be converted to a {conversion_type}. {} The argument provided was {arg}.", conversion_type.get_str("info").expect(CONVERSION_NO_INFO))]
pub struct ArgumentConversionError {
    pub arg_pos: usize,
    pub arg: String,
    pub conversion_type: ConversionType,
}

impl ArgumentConversionError {
    pub fn new(arg_pos: usize, arg: String, conversion_type: ConversionType) -> Self {
        Self {
            arg_pos,
            arg,
            conversion_type,
        }
    }
}

#[derive(Debug, Error)]
#[error("{serenity_errors:?}")]
pub struct SerenityErrors {
    pub serenity_errors: Vec<SerenityError>,
}

impl SerenityErrors {
    pub fn new(serenity_errors: Vec<SerenityError>) -> Self {
        SerenityErrors { serenity_errors }
    }
}

impl From<SerenityError> for SerenityErrors {
    fn from(error: SerenityError) -> Self {
        Self::new(vec![error])
    }
}

impl From<Vec<SerenityError>> for SerenityErrors {
    fn from(errors: Vec<SerenityError>) -> Self {
        Self::new(errors)
    }
}

#[derive(Error, Debug)]
pub enum SerenitySQLiteError {
    #[error("Serenity errors encountered: {0:?}")]
    SerenityError(#[from] SerenityErrors),
    #[error("SQLite error encountered: {0:?}")]
    SQLiteError(#[from] SQLiteError),
}

impl From<SerenityError> for SerenitySQLiteError {
    fn from(errors: SerenityError) -> Self {
        errors.into()
    }
}

impl From<Vec<SerenityError>> for SerenitySQLiteError {
    fn from(errors: Vec<SerenityError>) -> Self {
        errors.into()
    }
}
