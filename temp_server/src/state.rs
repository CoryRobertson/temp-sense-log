use crate::location::Location;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::fs::File;
use tokio::sync::Mutex;

pub struct TemperatureServerState {
    pub file_buf_list: Arc<Mutex<HashMap<Location, File>>>,
}

impl Default for TemperatureServerState {
    fn default() -> Self {
        Self {
            file_buf_list: Default::default(),
        }
    }
}
