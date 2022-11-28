use std::env;
use chrono::{DateTime, Datelike, Local, Timelike};
use std::fs::{create_dir_all, OpenOptions};
use std::io::{Read, Write};
use std::num::ParseFloatError;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use crate::EnvStatGetError::{ParseError, ParseErrorLength, UrlFailed};

fn main() {
    let args: Vec<String> = env::args().collect();

    let wifi = args.contains(&"wifi".to_string());

    let ports = serialport::available_ports().expect("no ports found");

    if wifi {
        let ip = {
            let mut output: String = String::new();
            for (index, arg) in args.clone().iter().enumerate() {
                if arg.contains("wifi") {
                    output = args.get(index + 1).unwrap().clone();
                }
            }
            output
        };
        println!("Running with wifi mode enabled");
        println!("IP to connect to: {}", ip);

        loop {
            let mut last_temp = 0.0;
            let mut last_humid = 0.0;
            let mut last_log = SystemTime::now();
            // let env_stat = match get_env_stat_from_url(&ip);
            match get_env_stat_from_url(&ip) {
                Ok(stat) => {
                    last_log = SystemTime::now();
                    last_temp = stat.temp;
                    last_humid = stat.humid;
                    println!("Temp: {}, Humid: {}", last_temp,last_humid);
                    print_stats_to_file(stat);
                }
                Err(err) => {
                    println!("error getting env stat from url: {:?}", err);
                    let stat = EnvStat{ temp: last_temp, humid: last_humid };
                    print_stats_to_file(stat);
                }
            }
            sleep(Duration::from_secs(5));
        }
    }
    else {
        println!("Running with wifi mode disabled, looking for compatible serial device");
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

}

#[derive(Debug)]
struct EnvStat {
    temp: f64,
    humid: f64,
}

#[derive(Debug)]
enum EnvStatGetError {
    UrlFailed,
    ParseError(ParseFloatError),
    ParseErrorLength,
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

fn get_env_stat_from_url(url: &str) -> Result<EnvStat,EnvStatGetError> {
    // let resp = reqwest::blocking::get(url)?.text()?;
    let resp = match reqwest::blocking::get(url) {
        Ok(res) => {
            match res.text() {
                Ok(text) => { text }
                Err(_) => { return Err(UrlFailed);}
            }
        }
        Err(_) => {
            return Err(UrlFailed);
        }
    };
    let split: Vec<&str> = resp.split(",").collect();

    let temp: f64 = match split.get(0) {
        None => { return Err(ParseErrorLength); }
        Some(tem) => {
            match tem.parse::<f64>() {
                Ok(t) => { t }
                Err(err) => { return Err(ParseError(err)); }
            }
        }
    };

    let humid: f64 = match split.get(1) {
        None => {
            return Err(ParseErrorLength);
        }
        Some(hum) => {
            match hum.parse::<f64>() {
                Ok(h) => { h }
                Err(err) => { return Err(ParseError(err)); }
            }
        }
    };

    Ok(EnvStat{ temp, humid })
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

    // file.sync_all().unwrap();

    match file.flush() {
        Ok(_) => {}
        Err(err) => { panic!("Error flushing file to system: {}", err); }
    }
}

#[cfg(test)]
mod tests {
    use crate::{get_timestamp_text, EnvStat, print_stats_to_file, get_env_stat_from_url};

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

    #[test]
    fn test_http_temp_sense_server() {

        let stat = get_env_stat_from_url("http://10.0.0.134:80").unwrap();
        println!("{:?}",stat);
        print_stats_to_file(stat);
    }

}
