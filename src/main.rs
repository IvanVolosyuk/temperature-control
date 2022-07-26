mod protos;

use core::result::Result;
use protobuf::Message;
use protos::generated::dev::LogMsg;
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

// std::net::SocketAddr

struct FragmentCombiner {
    hosts: HashMap<std::net::SocketAddr, Fragments>,
}

impl FragmentCombiner {
    fn add_fragment(
        &mut self,
        src: std::net::SocketAddr,
        buf: &[u8],
    ) -> Result<Option<LoggerProto>, String> {
        let client_address = src.to_string();

        if buf.len() < 5 {
            return Err("too short message".to_string());
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
            return Err(format!("bad magic: {}", info.magic));
        }

        if info.flags != 1 {
            return Err(format!("unsupported flags: {}", info.flags));
        }

        //src.ip.fold
        let curr = self.hosts.entry(src).or_insert_with(init_new_fragments);
        if curr.seq != info.seq {
            *curr = init_new_fragments();
            curr.seq = info.seq;
        }

        let begin = (info.curr as usize) * MAX_LOG_FRAGMENT;
        let end = begin + buf.len() - FRAG_INFO_SZ;

        if end > MAX_MESSAGE_SIZE {
            return Err(format!("message too large: {}\n", end));
        }

        if info.is_final {
            curr.last = Some(LastMessage {
                nfrag: info.curr + 1,
                total_size: end,
            });
        } else if buf.len() != MAX_UDP {
            return Err(format!(
                "{0}: wrong packet size: {1}\n",
                client_address,
                buf.len()
            ));
        }

        curr.message[begin..end].copy_from_slice(&buf[FRAG_INFO_SZ..buf.len()]);
        curr.recv_frag += 1;

        match &curr.last {
            None => Ok(None),
            Some(last) => {
                if last.nfrag == curr.recv_frag {
                    LoggerProto::parse_from_bytes(&curr.message[0..last.total_size].to_vec())
                        .map(|x| Some(x))
                        .map_err(|e| e.to_string())
                    //println!("log: {}", log.unwrap().to_string());
                } else {
                    Ok(None)
                }
            }
        }

        // println!("src: {:#?}", src.to_string());
    }
}

fn main() -> std::io::Result<()> {
    let mut msg = LogMsg::new();
    msg.set_ts(1);
    msg.set_text("foo".to_string());

    println!("Hello, world!");
    println!("proto: {0}", msg);

    let socket = UdpSocket::bind("192.168.0.1:6001")?;

    //let mut hosts = HashMap::new();
    let mut combiner = FragmentCombiner {
        hosts: HashMap::new(),
    };

    loop {
        // Receives a single datagram message on the socket. If `buf` is too small to hold
        // the message, it will be cut off.
        let mut buf = [0; MAX_UDP];
        let (sz, src) = socket.recv_from(&mut buf)?;
        //println!("{:#04X?}", buf);

        //let mut c = &combiner;
        let res = combiner.add_fragment(src, &buf[0..sz]);
        match res {
            Err(msg) => println!("{0}: ERROR: {1}", src.to_string(), msg),
            Ok(mb_proto) => match mb_proto {
                None => {}
                Some(proto) => println!("{0}: {1}", src.to_string(), proto.to_string()),
            },
        }
    }
    // Redeclare `buf` as slice of the received data and send reverse data back to origin.
    // let buf = &mut buf[..amt];
    // buf.reverse();
    // socket.send_to(buf, &src)?;
}
