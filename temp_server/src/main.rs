use actix_web::App;
use actix_web::HttpServer;
use actix_web::web;
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
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}

