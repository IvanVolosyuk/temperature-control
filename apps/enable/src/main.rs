use anyhow::{Result, bail};
use protobuf::Message;
use std::env;
use std::net::UdpSocket;
use temperature_protocol::protos::generated::dev::LoggerControl;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        bail!("Usage: host [(+|-)(store|send|serial|once|exp)] [restart]");
    }
    let mut c = LoggerControl::new();
    for arg in &args[2..] {
        if arg == "+serial" { c.set_log_to_serial(true); }
        else if arg == "-serial" { c.set_log_to_serial(false); }
        else if arg == "+store" { c.set_store_log(true); }
        else if arg == "-store" { c.set_store_log(false); }
        else if arg == "+send" { c.set_send_log(true); }
        else if arg == "-send" { c.set_send_log(false); }
        else if arg == "+once" { c.set_send_once(true); }
        else if arg == "-once" { c.set_send_once(false); }
        else if arg == "+exp" { c.set_experiment(true); }
        else if arg == "-exp" { c.set_experiment(false); }
        else if arg == "restart" { c.set_device_restart(true); }
        else { bail!("Unknown arg: {}", arg); }
    }

    let udp = UdpSocket::bind("0.0.0.0:0")?;
    let out_bytes: Vec<u8> = c.write_to_bytes()?;
    println!("Sending bytes: {:?}", out_bytes);
    udp.send_to(&out_bytes, args[1].to_owned() + ":6000")?;
    Ok(())
}
