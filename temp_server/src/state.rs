use crate::location::Location;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct TemperatureServerState {
    pub file_buf_list: Arc<Mutex<HashMap<Location, tokio::fs::File>>>,
}

impl Default for TemperatureServerState {
    fn default() -> Self {
        
        let mut hash_map = HashMap::new();
        
        match fs::read_dir("./") {
            Ok(dir) => {
                dir.into_iter()
                    .filter_map(|entry|{
                        entry
                            .ok()
                            .map(|entry_dir| {
                            match entry_dir.file_name().to_str() {
                                None => {
                                    None
                                }
                                Some(file_name) => {
                                    if file_name.contains(".csv") {
                                        Some(entry_dir)
                                    } else {None}
                                }
                            }
                        }).flatten()
                    })
                    .filter_map(|entry| entry.file_name().to_str().map(|name| (name.to_string(),entry.path())))
                    .for_each(|(csv_filename,entry_path)| {
                        hash_map.insert(csv_filename.replace(".csv","").into(),File::open(entry_path).unwrap().into());
                    });
            }
            Err(_) => {}
        }
        
        
        Self {
            file_buf_list: Arc::new(Mutex::new(hash_map)),
        }
    }
}
