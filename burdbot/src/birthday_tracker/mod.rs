mod birthday_manager;
mod birthday_server_role_manager;
mod role_updater;

pub use birthday_manager::*;
pub use birthday_server_role_manager::*;
pub use role_updater::*;

use crate::commands;

use chrono::{DateTime, Datelike, Timelike, Utc};
use lazy_static::lazy_static;
use regex::Regex;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef};
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

    fn one_day_ahead(&self) -> BirthdayDateTime {
        let mut day = (self.day + 1) % (commands::MONTH_TO_DAYS[self.month as usize] as u32 + 1);
        let mut month = self.month;

        if day == 0 {
            month = (self.month + 1) % 13;

            if month == 0 {
                month = 1;
            }

            day = 1;
        }

        BirthdayDateTime::new(month, day, self.hour)
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
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        let date_time_str = format!("{:02}-{:02}-{:02}", self.month, self.day, self.hour);

        Ok(ToSqlOutput::from(date_time_str))
    }
}

impl FromSql for BirthdayDateTime {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        lazy_static! {
            static ref BIRTHDAY_TIME_MATCHER: Regex = Regex::new(r"^(\d+)-(\d+)-(\d+)$").unwrap();
        }

        value.as_str().and_then(|sql_str| match BIRTHDAY_TIME_MATCHER.captures(sql_str) {
            Some(groups) => {
                let month = groups.get(1).unwrap().as_str().parse::<u32>().unwrap();
                let day = groups.get(2).unwrap().as_str().parse::<u32>().unwrap();
                let hour = groups.get(3).unwrap().as_str().parse::<u32>().unwrap();

                Ok(BirthdayDateTime { month, day, hour })
            }
            None => Err(FromSqlError::InvalidType),
        })
    }
}
