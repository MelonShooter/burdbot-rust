use reqwest::Error as ReqwestError;
use std::string::FromUtf8Error;
use thiserror::Error;

use base64::DecodeError;
use strum::ParseError;

#[derive(Debug, Copy, Clone)]
pub enum ForvoCaptureType {
    Base64,
    Country,
}

#[derive(Error, Debug, Copy, Clone)]
#[error("Couldn't match capture group {capture_group_idx} for {capture_type:?} from regex string: [ {regex_str} ].")]
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

#[non_exhaustive]
#[derive(Error, Debug, Clone)]
pub enum ForvoError {
    #[error("Error encountered while fetching forvo recordings. InvalidBase64: {0}")]
    InvalidBase64(#[from] DecodeError),
    #[error("Error encountered while fetching forvo recordings. InvalidUtf8Decode: {0}")]
    InvalidUtf8Decode(#[from] FromUtf8Error),
    #[error("Error encountered while fetching forvo recordings. BadBase64RegexMatching: {0}")]
    BadBase64RegexMatching(#[source] ForvoRegexCaptureError),
    #[error("Error encountered while fetching forvo recordings. BadCountryRegexMatching: {0}")]
    BadCountryRegexMatching(#[source] ForvoRegexCaptureError),
    #[error("Error encountered while fetching forvo recordings. InvalidMatchedCountry: {0}")]
    InvalidMatchedCountry(#[from] ParseError),
    #[error("Error encountered while fetching forvo recordings. ReqwestError: {0}")]
    ReqwestError(#[from] ReqwestError),
}

impl From<ForvoRegexCaptureError> for ForvoError {
    fn from(regex_capture_error: ForvoRegexCaptureError) -> Self {
        match regex_capture_error.capture_type {
            ForvoCaptureType::Base64 => Self::BadBase64RegexMatching(regex_capture_error),
            ForvoCaptureType::Country => Self::BadCountryRegexMatching(regex_capture_error),
        }
    }
}
