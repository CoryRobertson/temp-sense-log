use actix_web::{get, App, HttpResponseBuilder, Responder};
use actix_web::http::StatusCode;
use actix_web::HttpServer;
use actix_web::web;
use tracing::info;
use state::TemperatureServerState;
use crate::plotting_route::plot_location_handler;
use crate::reading_route::reading_handler;

mod location;
mod reading;
mod state;
mod plotting_route;
mod reading_route;

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

    let links = {

        let mut s = String::new();

        state.file_buf_list.lock().await.keys().cloned().for_each(|location| {
            let link = format!("/plot/{}",location.as_str());
            
            s.push_str(&format!(r###"<a href="{}">{}</a><br>"###,link,location.as_str()));
        });

        s
    };
    
    // TODO: show the last time a given sensor sent a reading
    
    info!("{}",links);

    HttpResponseBuilder::new(StatusCode::OK)
        .content_type("text/html")
        .body(links)
}