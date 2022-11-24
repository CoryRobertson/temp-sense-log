use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, SystemTime};
use chrono::{Datelike, DateTime, Local, Timelike};

fn main() {
    // println!("Hello, world!");
    let ports = serialport::available_ports().expect("no ports found");

    // let conv = "T:21.3:H:33.9:";



    for port in ports {
        // println!("{:?}", port);

        let mut com_port = serialport::new(port.port_name, 9600)
            .timeout(Duration::from_secs(5))
            .open()
            .expect("failed to read port");




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

                    if SystemTime::now().duration_since(last_log).unwrap().as_secs() > 60 {
                        last_log = SystemTime::now();
                        let stat = EnvStat{temp, humid};
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

fn print_stats_to_file(stat: EnvStat) {
    let path = Path::new("env_log.log");
    let display = path.display();
    let date: DateTime<Local> = Local::now();
    let file_name = "env_log.log";

    let mut file = match OpenOptions::new()
        .write(true)
        .create(true)
        .append(true)
        .open(file_name) {
        Ok(f) => {f}
        Err(e) => {
            panic!("{},{}", e, display);
        }
    };
    println!("{}",display);
    let am_pm = match date.hour12().0 {
        true => "PM",
        false => "AM",
    };

    let time_format = format!(
        "{}:{:02}:{:02}{}",
        date.hour12().1,
        date.minute(),
        date.second(),
        am_pm
    );

    let text = format!("Temperature: {}, Humidity: {}", stat.temp, stat.humid);

    let full_text = format!(
        "[{}-{}-{}: {}]\t{} \n",
        date.year(),
        date.month(),
        date.day(),
        time_format,
        text
    );

    match file.write_all(full_text.as_bytes()) {
        Ok(_) => {}
        Err(e) => {
            panic!("{}, {}", e, display);
        }
    };
}
