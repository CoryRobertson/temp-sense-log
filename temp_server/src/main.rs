use crate::plotting_route::plot_location_handler;
use crate::reading_route::reading_handler;
use actix_web::http::StatusCode;
use actix_web::web;
use actix_web::HttpServer;
use actix_web::{get, App, HttpResponseBuilder, Responder};
use chrono::Local;
use state::TemperatureServerState;
use std::path::PathBuf;
use std::string::ToString;
use std::sync::LazyLock;
use tracing::info;

mod location;
mod plotting_route;
mod reading;
mod reading_route;
mod state;

pub static LOG_FOLDER_PATH: LazyLock<PathBuf> =
    std::sync::LazyLock::new(|| PathBuf::from("./env_log"));

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let app_state = web::Data::new(TemperatureServerState::default());

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .service(reading_handler)
            .service(plot_location_handler)
            .service(main_page)
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}

#[get("/")]
async fn main_page(state: web::Data<TemperatureServerState>) -> impl Responder {
    let main_page_content = {
        let mut s = String::new();

        let lock = state.file_buf_list.lock().await;

        let location_info_list = lock
            .iter()
            .inspect(|(location, location_info)| {
                let link = format!("/plot/{}", location.as_str());

                let time_modified: Option<String> = location_info
                    .get_last_modified()
                    .map(|time| time.format("%m/%d/%Y %I:%M:%S %p").to_string());

                s.push_str(&format!(
                    r###"<a href="{}">{}</a> Last modified: {}<br>"###,
                    link,
                    location.as_str(),
                    time_modified.unwrap_or("Not modified".to_string())
                ));
            })
            .collect::<Vec<_>>();

        s.push_str("<br>");
        s.push_str("<br>");
        s.push_str("<br>");

        location_info_list
            .iter()
            .filter(|(_, location_info)| {
                location_info.get_last_modified().is_none()
                    || location_info
                        .get_last_modified()
                        .is_some_and(|last_modified| {
                            last_modified
                                .signed_duration_since(Local::now())
                                .num_minutes()
                                > 10
                        })
            })
            .for_each(|(location, location_info)| {
                let mia_sensor_text = format!(
                    "<b>MIA Sensor: {}, Last modified: {}</b><br>",
                    location.as_str(),
                    location_info
                        .get_last_modified()
                        .map(|time| time.format("%m/%d/%Y %I:%M:%S %p").to_string())
                        .unwrap_or("Not modified".to_string())
                );

                s.push_str(&mia_sensor_text);
            });

        s
    };

    info!("{}", main_page_content);

    HttpResponseBuilder::new(StatusCode::OK)
        .content_type("text/html")
        .body(main_page_content)
}
