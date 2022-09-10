use crate::protos::generated::dev::{RelayControl, RelayState};
use anyhow::Result;
use protobuf::Message;
use std::net::UdpSocket;

pub fn set_relay(addr: &str, on: bool, delay: u32) -> Result<()> {
    let udp = UdpSocket::bind("0.0.0.0:0")?;
    let mut msg: RelayControl = RelayControl::new();
    msg.set_dummy(true);
    msg.set_state(if on { RelayState::ON } else { RelayState::OFF });
    msg.set_delay(delay);
    let out_bytes: Vec<u8> = msg.write_to_bytes()?;
    udp.send_to(&out_bytes, addr.to_owned() + ":4210")?;
    Ok(())
}
