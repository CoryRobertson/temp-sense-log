use actix_web::{get, web, HttpResponseBuilder, Responder};
use tracing::{error, info};
use tokio::fs::OpenOptions;
use actix_web::http::StatusCode;
use tokio::io::AsyncWriteExt;
use crate::reading::Reading;
use crate::state::TemperatureServerState;

#[get("/reading/{location}/{temperature}/{humidity}")]
pub async fn reading_handler(
    reading: web::Path<(String, f32, f32)>,
    state: web::Data<TemperatureServerState>,
) -> impl Responder {
    let reading = Reading::from(reading);
    let file_format_data = reading.format_to_file();
    info!("New reading: {}", file_format_data);
    let location = reading.location();

    let mut lock = state.file_buf_list.lock().await;

    match lock.get_mut(&location) {
        None => {
            let file_already_exists = reading.path().exists();

            match OpenOptions::new()
                .append(true)
                .write(true)
                .read(true)
                .create(true) // TODO: this could be create_new(true) which would move us to error case if the file already exists, which would allow us to have possibly more clean code?
                .open(reading.path())
                .await
            {
                Ok(mut file) => {
                    if !file_already_exists {
                        // add file header for pretty-ness
                        file.write("Date,Time,Temperature,Humidity\n".as_bytes())
                            .await
                            .unwrap();
                        info!("Created new file for location: {}", location);
                    }

                    file.write(file_format_data.as_bytes()).await.unwrap();
                    info!("Wrote to file");
                    lock.insert(location.clone(), file);

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
            file.write(file_format_data.as_bytes()).await.unwrap();
            info!("Wrote to file");

            HttpResponseBuilder::new(StatusCode::OK).await.unwrap()
        }
    }
}