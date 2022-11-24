use std::io::Read;
use std::thread::sleep;
use std::time::Duration;

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
                    sleep(Duration::from_secs(1));
                }
                Err(_err) => {}
            }
        }
    }
}

