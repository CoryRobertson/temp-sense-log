use actix_web::web;
use actix_web::get;
use actix_web::App;
use actix_web::HttpServer;
use actix_web::Responder;
use tracing::info;


#[actix_web::main]
async fn main() -> std::io::Result<()> {

    tracing_subscriber::fmt::init();

    HttpServer::new(|| {
        App::new()
        .service(reading_handler)
    })
    .bind(("0.0.0.0",8080))?
    .run()
    .await
}



#[get("/reading/{location}/{temperature}/{humidity}")]
async fn reading_handler(
    reading: web::Path<(String, f32, f32)>,
) -> impl Responder {

    info!("YAY: {:?}", reading);


    format!("YAY: {:?}", reading)
}