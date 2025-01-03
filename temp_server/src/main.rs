
use tokio::fs::OpenOptions;
use actix_web::{web, HttpResponseBuilder};
use actix_web::get;
use actix_web::App;
use actix_web::http::StatusCode;
use actix_web::HttpServer;
use actix_web::Responder;
use tokio::io::{AsyncWriteExt};
use tracing::{error, info};
use reading::Reading;
use state::TemperatureServerState;

mod location;
mod reading;
mod state;

#[actix_web::main]
async fn main() -> std::io::Result<()> {

    tracing_subscriber::fmt::init();

    let app_state = web::Data::new(TemperatureServerState::default());

    HttpServer::new(move || {
        App::new()
        .app_data(app_state.clone())
        .service(reading_handler)
    })
    .bind(("0.0.0.0",8080))?
    .run()
    .await
}

#[get("/reading/{location}/{temperature}/{humidity}")]
async fn reading_handler(
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
                .create(true) // TODO: this could be create_new(true) which would move us to error case if the file already exists, which would allow us to have possibly more clean code?
                .open(reading.path()).await {
                Ok(mut file) => {

                    if !file_already_exists {
                        // add file header for pretty-ness
                        file.write("Date,Time,Temperature,Humidity\n".as_bytes()).await.unwrap();
                        info!("Created new file for location: {}", location);
                    }

                    file.write(file_format_data.as_bytes()).await.unwrap();
                    info!("Wrote to file");
                    lock.insert(location.clone(),file);

                    HttpResponseBuilder::new(StatusCode::CREATED)
                        .await.unwrap()
                }
                Err(err) => {
                    error!("Error opening file: {}", err);
                    HttpResponseBuilder::new(StatusCode::INTERNAL_SERVER_ERROR)
                        .await.unwrap()
                }
            }
        }
        Some(file) => {
            file.write(file_format_data.as_bytes()).await.unwrap();
            info!("Wrote to file");
            
            HttpResponseBuilder::new(StatusCode::OK)
                .await.unwrap()
        }
    }
}