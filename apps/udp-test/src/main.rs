use anyhow::{bail, Result};
use std::env;
use std::{thread::sleep, time::Duration};
use temperature_protocol::relay::set_relay;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        bail!("Usage: [host] (|1|0|undefined)");
    }
    if args.len() == 3 {
        let mode = match args[2].as_str() {
            "1" => true,
            "0" => false,
            "undefined" => bail!("Unsupported mode"),
            _ => bail!("Unknown mode: {}", args[2]),
        };
        set_relay(args[1].as_str(), mode, 0)?;
        println!("Set {} -> {}", args[2], args[1]);
    } else {
        set_relay(args[1].as_str(), true, 0)?;
        println!("Set on {}", args[1]);
        sleep(Duration::from_secs(1));
        set_relay(args[1].as_str(), false, 0)?;
        println!("Set off {}", args[1]);
    }
    Ok(())
}
