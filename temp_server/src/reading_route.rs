use crate::reading::Reading;
use crate::state::{LocationInfo, TemperatureServerState};
use crate::LOG_FOLDER_PATH;
use actix_web::http::StatusCode;
use actix_web::{get, web, HttpResponseBuilder, Responder};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::{error, info};

#[get("/reading/{location}/{temperature}/{humidity}")]
pub async fn reading_handler(
    reading: web::Path<(String, f32, f32)>,
    state: web::Data<TemperatureServerState>,
) -> impl Responder {
    let reading = Reading::from(reading);
    let file_format_data = reading.format_to_file();
    info!(
        "New reading: {} at location: {}",
        file_format_data,
        reading.location()
    );
    let location = reading.location();

    let mut lock = state.file_buf_list.lock().await;

    match lock.get_mut(&location) {
        None => {
            let file_path = LOG_FOLDER_PATH.join(reading.path());
            let file_already_exists = file_path.exists();

            match OpenOptions::new()
                .append(true)
                .write(true)
                .read(true)
                .create(true) // TODO: this could be create_new(true) which would move us to error case if the file already exists, which would allow us to have possibly more clean code?
                .open(file_path)
                .await
            {
                Ok(mut file) => {
                    if !file_already_exists {
                        // add file header for pretty-ness
                        let _ = file
                            .write("Date,Time,Temperature,Humidity\n".as_bytes())
                            .await
                            .unwrap();
                        info!("Created new file for location: {}", location);
                    }

                    let _ = file.write(file_format_data.as_bytes()).await.unwrap();
                    info!("Wrote to file");
                    lock.insert(location.clone(), LocationInfo::from(file));

                    HttpResponseBuilder::new(StatusCode::CREATED).await.unwrap()
                }
                Err(err) => {
                    error!("Error opening file: {}", err);
                    HttpResponseBuilder::new(StatusCode::INTERNAL_SERVER_ERROR)
                        .await
                        .unwrap()
                }
            }
        }
        Some(file) => {
            let _ = file
                .get_file_mut(true)
                .write(file_format_data.as_bytes())
                .await
                .unwrap();
            info!("Wrote to file");

            HttpResponseBuilder::new(StatusCode::OK).await.unwrap()
        }
    }
}
