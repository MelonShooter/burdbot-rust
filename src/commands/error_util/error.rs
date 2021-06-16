use rusqlite::Error as SQLiteError;
use serenity::Error as SerenityError;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;

#[derive(Debug)]
pub enum ArgumentParseErrorType<T: Ord + FromStr + Debug> {
    OutOfBounds(ArgumentOutOfBoundsError<T>),
    NotEnoughArguments(NotEnoughArgumentsError),
    ArgumentConversionError(ArgumentConversionError),
}

impl<T: Ord + FromStr + Debug> Display for ArgumentParseErrorType<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl<T: Ord + FromStr + Debug> Error for ArgumentParseErrorType<T> {}

#[derive(Debug)]
pub struct NotEnoughArgumentsError {
    pub min_args: u32,
    pub args_provided: u32,
}

impl NotEnoughArgumentsError {
    pub fn new(min_args: u32, args_provided: u32) -> NotEnoughArgumentsError {
        NotEnoughArgumentsError { min_args, args_provided }
    }
}

impl Display for NotEnoughArgumentsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl Error for NotEnoughArgumentsError {}

#[derive(Debug)]
pub struct ArgumentOutOfBoundsError<T: Ord + FromStr + Debug> {
    pub lower: T,
    pub upper: T,
    pub argument: T,
}

impl<T: Ord + FromStr + Debug> ArgumentOutOfBoundsError<T> {
    pub fn new(lower: T, upper: T, argument: T) -> ArgumentOutOfBoundsError<T> {
        ArgumentOutOfBoundsError { lower, upper, argument }
    }
}

impl<T: Ord + FromStr + Debug> Display for ArgumentOutOfBoundsError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl<T: Ord + FromStr + Debug> Error for ArgumentOutOfBoundsError<T> {}

#[derive(Debug)]
pub struct ArgumentConversionError {
    original_value: String,
}

impl ArgumentConversionError {
    pub fn new(original_value: String) -> ArgumentConversionError {
        ArgumentConversionError { original_value }
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
