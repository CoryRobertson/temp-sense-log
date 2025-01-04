use actix_web::{get, web, HttpResponseBuilder, Responder};
use tracing::{error, info};
use plotters::backend::SVGBackend;
use plotters::prelude::{Color, IntoFont, LineSeries, ShapeStyle, BLUE, GREEN, RED, WHITE};
use tokio::fs::OpenOptions;
use tracing::log::warn;
use plotters::chart::{ChartBuilder, SeriesLabelPosition};
use plotters::element::Rectangle;
use tokio::fs;
use std::str::from_utf8;
use actix_web::http::StatusCode;
use plotters::drawing::IntoDrawingArea;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use crate::state::TemperatureServerState;

#[get("/plot/{location}")]
pub async fn plot_location_handler(
    location: web::Path<String>,
    state: web::Data<TemperatureServerState>,
) -> impl Responder {
    let file_name = format!("{}.svg", location.as_str());

    info!("{}", file_name);

    let backend = SVGBackend::new(&file_name, (1000, 1000)).into_drawing_area();

    backend.fill(&WHITE).unwrap();

    let location = location.as_str().into();

    info!("Getting data");
    
    // TODO: this needs to eventually draw WAY more datapoints, as one is taken every minute, so this needs to scale all the points down quite a bit

    let (temperature_data, humidity_data) = {
        let mut lock = state.file_buf_list.lock().await;
        let string_data = {
            String::from_utf8(match lock.get_mut(&location) {
                None => {
                    match OpenOptions::new()
                        .append(true)
                        .write(true)
                        .read(true)
                        .create(true) // TODO: this could be create_new(true) which would move us to error case if the file already exists, which would allow us to have possibly more clean code?
                        .open(location.path())
                        .await
                    {
                        Ok(mut file) => {
                            let mut data = vec![];

                            file.read_to_end(&mut data).await.unwrap();

                            lock.insert(location.clone(), file);

                            data
                        }
                        Err(err) => {
                            error!("Error opening file: {}", err);
                            todo!()
                        }
                    }
                }
                Some(location_file) => {
                    location_file.rewind().await.unwrap();

                    let mut data = vec![];
                    location_file.read_to_end(&mut data).await.unwrap();

                    data
                }
            })
            .unwrap()
        };

        let lines = string_data
            .lines()
            .map(|s| {
                s.split(",")
                    .skip(1)
                    .map(|s| s.trim().to_string())
                    .collect::<Vec<String>>()
            })
            .collect::<Vec<Vec<String>>>();

        let mut temperature_data: Vec<(_, f32)> = vec![];
        let mut humidity_data: Vec<(_, f32)> = vec![];

        info!("len: {}", lines.len());
        for (idx, line) in lines.iter().enumerate() {
            // skip first line!
            if idx == 0 {
                continue;
            }

            match (
                line.get(0).map(|s| s.parse().ok()).flatten(),
                line.get(1).map(|s| s.parse().ok()).flatten(),
            ) {
                (Some(t), Some(h)) => {
                    temperature_data.push((idx as f32, t));
                    humidity_data.push((idx as f32, h));
                }
                _ => {
                    warn!("Bad line: {}: {:?}", idx, line);
                }
            }
        }

        // resize data arrays if they are longer than 100 data points for the graph
        if temperature_data.len() > 100 {
            loop {
                if temperature_data.len() <= 100 {
                    break;
                }
                temperature_data.remove(0);
            }

            let subtraction_amount = lines.len() - 100;

            let new_temperature_data = temperature_data
                .into_iter()
                
                .map(|(old_idx, temp)| ((old_idx - subtraction_amount as f32) as f32, temp))
                .collect();

            temperature_data = new_temperature_data;

            loop {
                if humidity_data.len() <= 100 {
                    break;
                }
                humidity_data.remove(0);
            }

            let new_humidity_data = humidity_data
                .into_iter()
                .map(|(old_idx, temp)| ((old_idx - subtraction_amount as f32) as f32, temp))
                .collect();

            humidity_data = new_humidity_data;
        }

        (temperature_data, humidity_data)
    };

    info!(
        "Data len: {}, {}",
        temperature_data.len(),
        humidity_data.len()
    );

    let highest_temp = temperature_data
        .iter()
        .map(|(_, temp)| *temp)
        .max_by(|temp, temp2| temp.total_cmp(temp2))
        .unwrap_or(100f32)
        .max(100f32);

    let mut chart = ChartBuilder::on(&backend)
        .caption(
            format!("Environmental Data for: {}", location.as_str()),
            ("sans-serif", 40).into_font(),
        )
        .x_label_area_size(20)
        .y_label_area_size(40)
        .build_cartesian_2d(0f32..100f32, 0f32..highest_temp)
        .unwrap();

    // Probably don't want to add an X Axis description since the time is arbitrary if we have scaled the data. For that, it is much more useful to simply use the CSV data
    // chart.configure_mesh().x_desc("Time").draw().unwrap();
    // A Y-Axis description would indeed be useful however
    chart
        .configure_mesh()
        .y_label_style(("sans-serif", 14).into_font())
        .y_desc("Humidity (%) / Temperature (F)")
        .axis_desc_style(("sans-serif", 14).into_font())
        .draw()
        .unwrap();

    info!("Got data");

    let temperature_style: ShapeStyle = RED.into();
    let humidity_style: ShapeStyle = GREEN.into();
    let stroke_width = 4;
    let point_size = 3;

    chart
        .draw_series(
            LineSeries::new(humidity_data, humidity_style.stroke_width(stroke_width))
                .point_size(point_size),
        )
        .unwrap()
        .label("Humidity")
        .legend(|(x, y)| Rectangle::new([(x - 15, y + 1), (x, y)], GREEN));

    chart
        .draw_series(
            LineSeries::new(
                temperature_data,
                temperature_style.stroke_width(stroke_width),
            )
            .point_size(point_size),
        )
        .unwrap()
        .label("Temperature")
        .legend(|(x, y)| Rectangle::new([(x - 15, y + 1), (x, y)], RED));

    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::UpperRight)
        .margin(20)
        .legend_area_size(5)
        .border_style(BLUE)
        .background_style(BLUE.mix(0.1))
        .label_font(("Calibri", 20))
        .draw()
        .unwrap();

    backend.present().unwrap();

    let file_data = fs::read(&file_name).await.unwrap();
    let content = from_utf8(&file_data).unwrap().to_string();

    HttpResponseBuilder::new(StatusCode::OK)
        // .append_header(("Content-Disposition", "inline"))
        .content_type("image/svg+xml")
        .body(content)
}