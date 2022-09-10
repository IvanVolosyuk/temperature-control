use anyhow::Result;
use std::collections::HashMap;
use temperature_protocol::fragment_combiner::FragmentCombiner;
use temperature_protocol::fragment_combiner::MessageHandler;
use temperature_protocol::protos::generated::dev::DeviceMessage;

struct Server {
    _hosts: HashMap<std::net::SocketAddr, i64>,
}

impl Server {
    fn new() -> Server {
        Server {
            _hosts: HashMap::new(),
        }
    }
}

impl MessageHandler<DeviceMessage> for Server {
    fn on_message(
        &mut self,
        _src: std::net::SocketAddr,
        _msg: DeviceMessage,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

fn main() -> Result<()> {
    let mut server = Server::new();
    FragmentCombiner::new(&mut server).main_loop("0.0.0.0:4000")
}
