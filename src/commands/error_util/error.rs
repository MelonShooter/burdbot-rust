use rusqlite::Error as SQLiteError;
use serenity::Error as SerenityError;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

#[derive(Debug)]
pub enum ArgumentParseErrorType {
    OutOfBounds(ArgumentOutOfBoundsError),
    NotEnoughArguments(NotEnoughArgumentsError),
    ArgumentConversionError(ArgumentConversionError),
    BadOption(BadOptionError),
}

impl Display for ArgumentParseErrorType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl Error for ArgumentParseErrorType {}

#[derive(Debug, Clone)]
pub struct BadOptionError {
    pub arg_pos: usize,
    pub choices: String,
}

impl BadOptionError {
    pub(in crate::commands) fn new(arg_pos: usize, choices: String) -> Self {
        Self { arg_pos, choices }
    }
}

impl Display for BadOptionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl Error for BadOptionError {}

#[derive(Debug)]
pub struct NotEnoughArgumentsError {
    pub min_args: usize,
    pub args_provided: usize,
}

impl NotEnoughArgumentsError {
    pub fn new(min_args: usize, args_provided: usize) -> Self {
        Self { min_args, args_provided }
    }
}

impl Display for NotEnoughArgumentsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl Error for NotEnoughArgumentsError {}

#[derive(Debug)]
pub struct ArgumentOutOfBoundsError {
    pub lower: i64,
    pub upper: i64,
    pub argument: i64,
}

impl ArgumentOutOfBoundsError {
    pub fn new(lower: i64, upper: i64, argument: i64) -> Self {
        Self { lower, upper, argument }
    }
}

impl Display for ArgumentOutOfBoundsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl Error for ArgumentOutOfBoundsError {}

#[derive(Debug)]
pub struct ArgumentConversionError {
    _original_value: String,
}

impl ArgumentConversionError {
    pub fn new(original_value: String) -> Self {
        Self {
            _original_value: original_value,
        }
    }
}

impl Display for ArgumentConversionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl Error for ArgumentConversionError {}

#[derive(Debug)]
pub enum SerenitySQLiteError {
    SerenityError(Vec<SerenityError>),
    SQLiteError(SQLiteError),
}

impl From<Vec<SerenityError>> for SerenitySQLiteError {
    fn from(errors: Vec<SerenityError>) -> Self {
        SerenitySQLiteError::SerenityError(errors)
    }
}

impl From<SerenityError> for SerenitySQLiteError {
    fn from(errors: SerenityError) -> Self {
        SerenitySQLiteError::SerenityError(vec![errors])
    }
}

impl From<SQLiteError> for SerenitySQLiteError {
    fn from(error: SQLiteError) -> Self {
        SerenitySQLiteError::SQLiteError(error)
    }
}

impl Display for SerenitySQLiteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl Error for SerenitySQLiteError {}
