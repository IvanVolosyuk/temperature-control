mod protos;

use anyhow::{bail, Result};
// use protos::generated::dev::LogMsg;
use protos::generated::dev::LoggerProto;
use std::collections::HashMap;
use std::net::UdpSocket;

#[derive(Debug)]
struct FragInfo {
    magic: u8,
    flags: u8,
    seq: u8,
    is_final: bool,
    curr: u8,
}

const FRAG_MAGIC: u8 = 0xfa;
const MAX_MESSAGE_SIZE: usize = 65536;
const MAX_UDP: usize = 1460;
const FRAG_INFO_SZ: usize = 5;
const MAX_LOG_FRAGMENT: usize = MAX_UDP - FRAG_INFO_SZ;

struct LastMessage {
    nfrag: u8,
    total_size: usize,
}

struct Fragments {
    seq: u8,
    recv_frag: u8,
    last: Option<LastMessage>,
    message: [u8; MAX_MESSAGE_SIZE],
}

fn init_new_fragments() -> Fragments {
    Fragments {
        seq: 0,
        recv_frag: 0,
        last: None,
        message: [0; MAX_MESSAGE_SIZE],
    }
}

trait MessageHandler<T> {
    fn on_message(&mut self, src: std::net::SocketAddr, msg: T) -> anyhow::Result<()>;
}

struct LogPrinter {}

impl MessageHandler<LoggerProto> for LogPrinter {
    fn on_message(&mut self, src: std::net::SocketAddr, msg: LoggerProto) -> anyhow::Result<()> {
        println!("{0}: {1}", src.to_string(), msg.to_string());
        Ok(())
    }
}

struct FragmentCombiner<'a, T> {
    hosts: HashMap<std::net::SocketAddr, Fragments>,
    handler: &'a mut dyn MessageHandler<T>,
}

impl<T: protobuf::Message> FragmentCombiner<'_, T> {
    fn new(handler: &mut dyn MessageHandler<T>) -> FragmentCombiner<T> {
        FragmentCombiner {
            hosts: HashMap::new(),
            handler,
        }
    }

    fn main_loop(&mut self, bind_addr : &str) -> anyhow::Result<()> {
        let socket = UdpSocket::bind(bind_addr)?;

        loop {
            let mut buf = [0; MAX_UDP];
            let (sz, src) = socket.recv_from(&mut buf)?;

            let res = self.add_fragment(src, &buf[0..sz]);
            match res {
                Err(msg) => println!("{0}: ERROR: {1:?}", src.to_string(), msg),
                Ok(()) => (),
            }
        }
    }

    fn add_fragment(&mut self, src: std::net::SocketAddr, buf: &[u8]) -> Result<()> {
        if buf.len() < 5 {
            bail!("too short message, len: {}", buf.len());
        }

        let info = FragInfo {
            magic: buf[0],
            flags: buf[1],
            seq: buf[2],
            is_final: buf[3] != 0,
            curr: buf[4],
        };
        println!("info: {:#?}", info);

        if info.magic != FRAG_MAGIC {
            bail!("bad magic: {}", info.magic);
        }

        if info.flags != 1 {
            bail!("unsupported flags: {}", info.flags);
        }

        let curr = self.hosts.entry(src).or_insert_with(init_new_fragments);
        if curr.seq != info.seq {
            *curr = init_new_fragments();
            curr.seq = info.seq;
        }

        let begin = (info.curr as usize) * MAX_LOG_FRAGMENT;
        let end = begin + buf.len() - FRAG_INFO_SZ;

        if end > MAX_MESSAGE_SIZE {
            bail!("message too large: {}\n", end);
        }

        if info.is_final {
            curr.last = Some(LastMessage {
                nfrag: info.curr + 1,
                total_size: end,
            });
        } else if buf.len() != MAX_UDP {
            bail!("wrong packet size: {}\n", buf.len());
        }

        curr.message[begin..end].copy_from_slice(&buf[FRAG_INFO_SZ..buf.len()]);
        curr.recv_frag += 1;

        match &curr.last {
            None => {}
            Some(last) => {
                if last.nfrag == curr.recv_frag {
                    let message = T::parse_from_bytes(&curr.message[0..last.total_size].to_vec())?;
                    self.handler.on_message(src, message)?;
                }
            }
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    let mut log = LogPrinter {};
    FragmentCombiner::new(&mut log).main_loop("192.168.0.1:6001")
}
