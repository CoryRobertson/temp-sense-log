use crate::location::Location;
use actix_web::web;
use actix_web::web::Path;
use chrono::{DateTime, Local};
use std::path::PathBuf;

pub struct Reading {
    location: Location,
    temperature: f32,
    humidity: f32,
    reading_time: DateTime<Local>,
}

impl Reading {
    pub fn location(&self) -> Location {
        self.location.as_str().into()
    }

    pub fn temperature(&self) -> f32 {
        self.temperature
    }

    pub fn humidity(&self) -> f32 {
        self.humidity
    }

    pub fn path(&self) -> PathBuf {
        self.location.path()
    }

    pub fn format_to_file(&self) -> String {
        let date_string = format!("{}", self.reading_time.format("%m/%d/%Y"));
        let time_string = format!("{}", self.reading_time.format("%I:%M:%S %p"));

        format!(
            "{} {},{},{}\n",
            date_string,
            time_string,
            self.temperature(),
            self.humidity()
        )
    }

    pub fn reading_time(&self) -> DateTime<Local> {
        self.reading_time
    }
}

impl From<web::Path<(String, f32, f32)>> for Reading {
    fn from(value: Path<(String, f32, f32)>) -> Self {
        let value = value.into_inner();
        Self {
            location: value.0.into(),
            temperature: {
                // We convert the reading to Fahrenheit since the sensor itself spits out Celcius measurements.
                (value.1 * 1.8) + 32.0
            },
            humidity: value.2,
            reading_time: Local::now(),
        }
    }
}
