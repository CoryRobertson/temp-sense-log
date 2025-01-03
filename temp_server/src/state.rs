use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use tokio::fs::File;
use crate::location::Location;

pub struct TemperatureServerState {
    pub file_buf_list: Arc<Mutex<HashMap<Location,File>>>,
}

impl Default for TemperatureServerState {
    fn default() -> Self {
        Self {
            file_buf_list: Default::default(),
        }
    }
}