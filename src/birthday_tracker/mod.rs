mod birthday_manager;
mod birthday_server_role_manager;
mod role_updater;

pub use birthday_manager::*;
pub use birthday_server_role_manager::*;
use chrono::{DateTime, Datelike, Timelike, Utc};
use lazy_static::lazy_static;
use regex::Regex;
pub use role_updater::*;
use rusqlite::types::FromSql;
use rusqlite::types::FromSqlError;
use rusqlite::types::ToSqlOutput;
use rusqlite::ToSql;

#[derive(Clone, Copy)]
struct BirthdayDateTime {
    month: u32,
    day: u32,
    hour: u32,
}

impl BirthdayDateTime {
    fn new(month: u32, day: u32, hour: u32) -> BirthdayDateTime {
        BirthdayDateTime { month, day, hour }
    }
}

impl From<DateTime<Utc>> for BirthdayDateTime {
    fn from(date_time: DateTime<Utc>) -> Self {
        BirthdayDateTime {
            month: date_time.month(),
            day: date_time.day(),
            hour: date_time.hour(),
        }
    }
}

impl ToSql for BirthdayDateTime {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        let date_time_str = format!("{}-{}-{}", self.month, self.day, self.hour);

        Ok(ToSqlOutput::from(date_time_str))
    }
}

impl FromSql for BirthdayDateTime {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        lazy_static! {
            static ref BIRTHDAY_TIME_MATCHER: Regex = Regex::new(r"^(\d+)-(\d+)-(\d+)$").unwrap();
        }

        value.as_str().and_then(|sql_str| match BIRTHDAY_TIME_MATCHER.captures(sql_str) {
            Some(groups) => {
                let month = groups.get(0).unwrap().as_str().parse::<u32>().unwrap();
                let day = groups.get(1).unwrap().as_str().parse::<u32>().unwrap();
                let hour = groups.get(2).unwrap().as_str().parse::<u32>().unwrap();

                Ok(BirthdayDateTime { month, day, hour })
            }
            None => Err(FromSqlError::InvalidType),
        })
    }
}
