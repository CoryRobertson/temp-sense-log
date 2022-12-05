use crate::EnvStatGetError::{ParseError, ParseErrorLength, UrlFailed};
use chrono::{DateTime, Datelike, Local, Timelike};
use std::fs::{create_dir_all, OpenOptions};
use std::io::{Read, Write};
use std::num::ParseFloatError;
use std::path::Path;
use std::thread::{sleep, JoinHandle};
use std::time::{Duration, SystemTime};
use std::{env, fs, thread};

static POLL_DELAY_SECONDS: u64 = 5;

fn main() {
    let args: Vec<String> = env::args().collect();

    let wifi = args.contains(&"wifi".to_string());

    let ports = serialport::available_ports().expect("no ports found");

    if wifi {
        let devices = read_device_list(); // devices are output in a vector in form of (ip, device name)
        let mut threads: Vec<JoinHandle<()>> = vec![];
        println!("Running with wifi mode enabled");

        for device in devices {
            threads.push(spawn_stat_connection_thread(device.0, device.1));
        } // add each device from the device list to its own thread.

        for thread in threads {
            let _ = thread.join();
        } // join all threads.

        //TODO: needs testing, waiting on pico w boards.
    } else {
        println!("Running with wifi mode disabled, looking for compatible serial device");
        for port in ports {

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
                            print_stats_to_file(stat, "env_log.csv");
                        }
                    }
                    Err(_err) => {}
                }
            }
        }
    }
}

fn spawn_stat_connection_thread(ip: String, device_name: String) -> JoinHandle<()> {
    thread::spawn(move || {
        let ip = ip;
        let device_name = device_name;
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
                    println!("Device Name: {}, Temp: {}, Humid: {}", &device_name, last_temp, last_humid);
                    print_stats_to_file(stat, &device_name);
                }
                Err(err) => {
                    println!("error getting env stat from url: {:?}", err);
                    let stat = EnvStat {
                        temp: last_temp,
                        humid: last_humid,
                    };
                    print_stats_to_file(stat, &device_name);
                    break; // stop the thread once we are no longer able to get data from it, potentially allow this to be a run option???
                }
            }
            sleep(Duration::from_secs(POLL_DELAY_SECONDS));
        }
    })
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

fn get_env_stat_from_url(url: &str) -> Result<EnvStat, EnvStatGetError> {
    // let resp = reqwest::blocking::get(url)?.text()?;
    let resp = match reqwest::blocking::get(url) {
        Ok(res) => match res.text() {
            Ok(text) => text,
            Err(_) => {
                return Err(UrlFailed);
            }
        },
        Err(_) => {
            return Err(UrlFailed);
        }
    };
    let split: Vec<&str> = resp.split(",").collect();

    let temp: f64 = match split.get(0) {
        None => {
            return Err(ParseErrorLength);
        }
        Some(tem) => match tem.parse::<f64>() {
            Ok(t) => t,
            Err(err) => {
                return Err(ParseError(err));
            }
        },
    };

    let humid: f64 = match split.get(1) {
        None => {
            return Err(ParseErrorLength);
        }
        Some(hum) => match hum.parse::<f64>() {
            Ok(h) => h,
            Err(err) => {
                return Err(ParseError(err));
            }
        },
    };

    Ok(EnvStat { temp, humid })
}

fn print_stats_to_file(stat: EnvStat, device_name: &str) {
    // let path_with_filename = Path::new("./log/env_log.csv");
    let file_path_name = format!("./log/{}.csv", device_name);
    let path_with_filename = Path::new(&file_path_name);
    let path_without_filename = Path::new("./log/");
    let display = path_with_filename.display();
    // let file_name = "env_log.csv";

    match create_dir_all(path_without_filename) {
        Ok(_) => {}
        Err(err) => {
            panic!(
                "Could not create directories: '{}' in path_with_filename: {}",
                err, display
            );
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

    let full_text = get_timestamp_text(&stat);

    match file.write_all(full_text.as_bytes()) {
        Ok(_) => {}
        Err(e) => {
            panic!("{}, {}", e, display);
        }
    };

    match file.flush() {
        Ok(_) => {}
        Err(err) => {
            panic!("Error flushing file to system: {}", err);
        }
    }
}

fn read_device_list() -> Vec<(String, String)> {
    let path = Path::new("./devices.csv");
    if let Ok(contents) = fs::read_to_string(path) {
        #[cfg(debug_assertions)]
        println!("DEBUG: FILE CONTENTS READ: {}", contents);

        let list: Vec<&str> = contents.split(",").collect();
        let mut return_vec: Vec<(String, String)> = vec![];
        let mut iter = list.iter();
        loop {
            if iter.len() == 0 {
                break;
            } // if iterator is empty, we break the loop
            let ip = match iter.next() {
                None => {
                    return return_vec; // if the iterator wasn't empty, this should never fail, but just incase :)
                }
                Some(ip) => ip.clone(),
            };
            let device_name = match iter.next() {
                None => {
                    println!("[Warning] devices.csv file missing name for an ip, found an ip but not a name, skipping...");
                    return return_vec; // if the device list was missing a name for this ip address, we break early and skip it.
                }
                Some(devname) => devname.clone(),
            };
            return_vec.push((ip.to_string(), device_name.to_string()));
        }
        return_vec // return the list we made if we found the file
    } else {
        return vec![]; // if we didnt find the file, return an empty list, most likely making the program run nothing and cease.
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        get_env_stat_from_url, get_timestamp_text, print_stats_to_file, read_device_list, EnvStat,
    };

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
        print_stats_to_file(stat, "env_log.csv");
    }

    #[test]
    fn test_http_temp_sense_server() {
        let stat = get_env_stat_from_url("http://10.0.0.134:80").unwrap();
        println!("{:?}", stat);
        print_stats_to_file(stat, "env_log.csv");
    }

    #[test]
    fn test_read_device_list() {
        let list = read_device_list();
        for (ip, device_name) in list {
            println!("IP: {}, Device name: {}", &ip, &device_name);
        }
    }
}
