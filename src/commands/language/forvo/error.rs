use reqwest::Error as ReqwestError;
use std::error::Error;
use std::fmt::Display;
use std::string::FromUtf8Error;

use base64::DecodeError;
use strum::ParseError;

#[derive(Debug)]
pub enum ForvoCaptureType {
    Base64,
    Country,
}

#[derive(Debug)]
pub struct ForvoRegexCaptureError {
    regex_str: &'static str,
    capture_group_idx: usize,
    capture_type: ForvoCaptureType,
}

impl ForvoRegexCaptureError {
    pub fn new(regex_str: &'static str, capture_group_idx: usize, capture_type: ForvoCaptureType) -> Self {
        Self {
            regex_str,
            capture_group_idx,
            capture_type,
        }
    }
}

impl Display for ForvoRegexCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Couldn't match capture group {} from regex string: [ {} ].",
            self.capture_group_idx, self.regex_str
        )
    }
}

impl Error for ForvoRegexCaptureError {}

#[non_exhaustive]
#[derive(Debug)]
pub enum ForvoError {
    InvalidBase64(DecodeError),
    InvalidUtf8Decode(FromUtf8Error),
    BadBase64RegexMatching(ForvoRegexCaptureError),
    BadCountryRegexMatching(ForvoRegexCaptureError),
    InvalidMatchedCountry(ParseError),
    ReqwestError(ReqwestError),
}

impl From<DecodeError> for ForvoError {
    fn from(decode_error: DecodeError) -> Self {
        Self::InvalidBase64(decode_error)
    }
}

impl From<FromUtf8Error> for ForvoError {
    fn from(from_utf8_error: FromUtf8Error) -> Self {
        Self::InvalidUtf8Decode(from_utf8_error)
    }
}

impl From<ForvoRegexCaptureError> for ForvoError {
    fn from(regex_capture_error: ForvoRegexCaptureError) -> Self {
        match regex_capture_error.capture_type {
            ForvoCaptureType::Base64 => Self::BadBase64RegexMatching(regex_capture_error),
            ForvoCaptureType::Country => Self::BadCountryRegexMatching(regex_capture_error),
        }
    }
}

impl From<ParseError> for ForvoError {
    fn from(parse_error: ParseError) -> Self {
        Self::InvalidMatchedCountry(parse_error)
    }
}

impl From<ReqwestError> for ForvoError {
    fn from(reqwest_error: ReqwestError) -> Self {
        Self::ReqwestError(reqwest_error)
    }
}

impl Display for ForvoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = "Error encountered while fetching forvo recordings. ";

        match self {
            ForvoError::InvalidBase64(err) => write!(f, "{message} InvalidBase64: {err}"),
            ForvoError::InvalidUtf8Decode(err) => write!(f, "{message} InvalidUtf8Error: {err}"),
            ForvoError::BadBase64RegexMatching(err) => write!(f, "{message} BadBase64RegexMatchingError: {err}"),
            ForvoError::BadCountryRegexMatching(err) => write!(f, "{message} BadCountryRegexMatching: {err}"),
            ForvoError::InvalidMatchedCountry(err) => write!(f, "{message} InvalidMatchedCountry: {err}"),
            ForvoError::ReqwestError(err) => write!(f, "{message} ReqwestError: {err}"),
        }
    }
}

impl Error for ForvoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ForvoError::InvalidBase64(err) => Some(err),
            ForvoError::InvalidUtf8Decode(err) => Some(err),
            ForvoError::BadBase64RegexMatching(err) => Some(err),
            ForvoError::BadCountryRegexMatching(err) => Some(err),
            ForvoError::InvalidMatchedCountry(err) => Some(err),
            ForvoError::ReqwestError(err) => Some(err),
        }
    }
}
