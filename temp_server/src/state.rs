use crate::location::Location;
use crate::LOG_FOLDER_PATH;
use chrono::{DateTime, Local};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::sync::Mutex;

pub struct TemperatureServerState {
    pub file_buf_list: Arc<Mutex<HashMap<Location, LocationInfo>>>,
}

pub struct LocationInfo {
    file: tokio::fs::File,
    last_modified: Option<DateTime<Local>>,
}

impl From<File> for LocationInfo {
    fn from(file: File) -> Self {
        Self {
            file: file.into(),
            last_modified: None,
        }
    }
}

impl From<tokio::fs::File> for LocationInfo {
    fn from(file: tokio::fs::File) -> Self {
        Self {
            file,
            last_modified: None,
        }
    }
}

impl LocationInfo {
    pub fn get_file_mut(&mut self, update_last_modified: bool) -> &mut tokio::fs::File {
        if update_last_modified {
            self.last_modified = Some(Local::now());
        }
        &mut self.file
    }

    pub fn get_last_modified(&self) -> Option<&DateTime<Local>> {
        self.last_modified.as_ref()
    }
}

impl Default for TemperatureServerState {
    fn default() -> Self {
        let mut hash_map = HashMap::new();

        match fs::read_dir(LOG_FOLDER_PATH.clone()) {
            Ok(dir) => {
                dir.into_iter()
                    .filter_map(|entry| {
                        entry
                            .ok()
                            .and_then(|entry_dir| match entry_dir.file_name().to_str() {
                                None => None,
                                Some(file_name) => {
                                    if file_name.contains(".csv") {
                                        Some(entry_dir)
                                    } else {
                                        None
                                    }
                                }
                            })
                    })
                    .filter_map(|entry| {
                        entry
                            .file_name()
                            .to_str()
                            .map(|name| (name.to_string(), entry.path()))
                    })
                    .for_each(|(csv_filename, entry_path)| {
                        hash_map.insert(
                            csv_filename.replace(".csv", "").into(),
                            std::fs::OpenOptions::new()
                                .append(true)
                                .write(true)
                                .read(true)
                                .create(true) // TODO: this could be create_new(true) which would move us to error case if the file already exists, which would allow us to have possibly more clean code?
                                .open(entry_path).unwrap().into(),
                        );
                    });
            }
            Err(_) => {}
        }

        Self {
            file_buf_list: Arc::new(Mutex::new(hash_map)),
        }
    }
}
