use chrono::{DateTime, Datelike, Local, Timelike};
use std::fs::{create_dir_all, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

fn main() {
    // println!("Hello, world!");
    let ports = serialport::available_ports().expect("no ports found");

    // let conv = "T:21.3:H:33.9:";

    for port in ports {
        // println!("{:?}", port);

        let mut com_port = serialport::new(port.port_name.clone(), 9600)
            .timeout(Duration::from_secs(5))
            .open()
            .expect("failed to read port");

        println!("{}", port.port_name);

        let mut temp = 0.0;
        let mut humid = 0.0;
        let mut last_log = SystemTime::now();

        loop {
            let mut serial_buf: Vec<u8> = vec![0; 32];
            com_port
                .read(serial_buf.as_mut_slice())
                .expect("failed to read from port");
            let convert_buf = String::from_utf8(serial_buf.clone());
            match convert_buf {
                Ok(conv) => {
                    // println!("begin conv");
                    // println!("{}", conv);
                    // println!("{:?}", serial_buf);
                    // println!("end conv");

                    if let Some(start) = conv.find("T:") {
                        let t = conv[start + 2..].to_string();
                        if let Some(end) = t.find(":") {
                            temp = t[..end].to_string().parse().unwrap();
                        }
                    }

                    if let Some(start) = conv.find("H:") {
                        let h = conv[start + 2..].to_string();
                        if let Some(end) = h.find(":") {
                            humid = h[..end].to_string().parse().unwrap();
                        }
                    }
                    println!("Temp: {}", temp);
                    println!("Humid: {}", humid);
                    println!();
                    sleep(Duration::from_secs(1));

                    if SystemTime::now()
                        .duration_since(last_log)
                        .unwrap()
                        .as_secs()
                        > 60
                    {
                        last_log = SystemTime::now();
                        let stat = EnvStat { temp, humid };
                        print_stats_to_file(stat);
                    }
                }
                Err(_err) => {}
            }
        }
    }
}

struct EnvStat {
    temp: f64,
    humid: f64,
}

fn get_timestamp_text(stat: &EnvStat) -> String {
    let date: DateTime<Local> = Local::now();
    // [2022-11-26: 5:19:53PM] Temperature: 24.4, Humidity: 31 old format
    // 11/26/2022,5:19:53PM,24.4,31.0 new format
    let am_pm = match date.hour12().0 {
        true => "PM",
        false => "AM",
    };
    let month_day_year = format!("{}/{}/{}", date.month(), date.day(), date.year());
    let time_format = format!(
        "{}:{:02}:{:02} {}",
        date.hour12().1,
        date.minute(),
        date.second(),
        am_pm
    );
    let full_text = format!(
        "{} {},{},{}\n",
        month_day_year, time_format, stat.temp, stat.humid,
    );
    full_text
}

fn print_stats_to_file(stat: EnvStat) {
    let path_with_filename = Path::new("./log/env_log.csv");
    let path_without_filename = Path::new("./log/");
    let display = path_with_filename.display();
    // let file_name = "env_log.csv";

    match create_dir_all(path_without_filename) {
        Ok(_) => {}
        Err(err) => {
            panic!("Could not create directories: '{}' in path_with_filename: {}",err,display);
        }
    }

    let mut file = match OpenOptions::new()
        .write(true)
        .create(true)
        .append(true)
        .open(path_with_filename)
    {
        Ok(f) => f,
        Err(e) => {
            panic!("{},{}", e, display);
        }
    };
    println!("{}", display);

    let full_text = get_timestamp_text(&stat);

    match file.write_all(full_text.as_bytes()) {
        Ok(_) => {}
        Err(e) => {
            panic!("{}, {}", e, display);
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::{get_timestamp_text, EnvStat, print_stats_to_file};

    #[test]
    fn test_get_timestamp_text() {
        let stat = EnvStat {
            temp: 24.3,
            humid: 30.1,
        };
        print!("{}", get_timestamp_text(&stat));
    }

    #[test]
    fn test_create_log_file() {
        let stat = EnvStat {
            temp: 24.3,
            humid: 30.1,
        };
        print_stats_to_file(stat);
    }
}
