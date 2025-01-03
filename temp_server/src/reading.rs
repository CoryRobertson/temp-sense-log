use actix_web::web;
use actix_web::web::Path;
use chrono::{DateTime, Local};
use std::path::PathBuf;
use crate::location::Location;

pub struct Reading {
    location: String,
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
        PathBuf::from(format!("{}.csv",self.location))
    }

    pub fn format_to_file(&self) -> String {
        
        let date_string = format!("{}", self.reading_time.format("%m-%d-%Y"));
        let time_string = format!("{}", self.reading_time.format("%I:%M:%S %p"));
        
        format!("{},{},{},{}\n",date_string,time_string,self.temperature(),self.humidity())
    }

    pub fn reading_time(&self) -> DateTime<Local> {
        self.reading_time
    }
}

impl From<web::Path<(String, f32, f32)>> for Reading {
    fn from(value: Path<(String, f32, f32)>) -> Self {
        let value = value.into_inner();
        Self {
            location: value.0,
            temperature: value.1,
            humidity: value.2,
            reading_time: Local::now(),
        }
    }
}