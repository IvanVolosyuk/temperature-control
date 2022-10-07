use anyhow::Result;
use chrono::DateTime;
use chrono::{Duration, Local};
use temperature_protocol::protos::generated::dev::LoggerProto;
use std::net::IpAddr;
use std::{collections::HashMap, io::Write};
use temperature_protocol::fragment_combiner::FragmentCombiner;
use temperature_protocol::fragment_combiner::MessageHandler;
use std::sync::mpsc;
use std::thread;

struct LogPrinter {
    hosts: HashMap<std::net::SocketAddr, u64>,
    last_host: std::net::IpAddr,
    sender: mpsc::Sender<std::net::IpAddr>,
    receiver: mpsc::Receiver<i8>,
}

fn maybe_print_header(last_host: &mut std::net::IpAddr, src: std::net::IpAddr, curr_ts: u64) {
    if *last_host != src {
        let mins = curr_ts / 60000 % 60;
        let hours = curr_ts / 3600000 % 24;
        let days = curr_ts / 86400000;
        println!(
            "===== {} (up {}d {}h {}m) ======",
            src.to_string(),
            days,
            hours,
            mins
        );
        *last_host = src;
    }
}



impl LogPrinter {
    fn new() -> LogPrinter {
        let (tx, rx) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();

        thread::spawn(move || {
            let mut deadlines : HashMap<IpAddr, DateTime<Local>> = HashMap::new();
            loop {
                let time = Local::now();
                let mut timeout = Duration::minutes(5);

                deadlines.retain(|ip, deadline| {

                    let dt = deadline.signed_duration_since(time);
                    let expired = dt.le(&Duration::zero());
                    if expired {
                        print!(
                            "===== {0} OFFLINE ({1})\n",
                            ip.to_string(),
                            time.format("%a %d %b %H:%M:%S"));
                    }

                    if dt < timeout { timeout = dt }
                    !expired
                });

                if timeout.gt(&Duration::zero()) {
                    match rx.recv_timeout(timeout.to_std().unwrap()) {
                        Ok(ip)=> {
                            let prev = deadlines.insert(ip, Local::now() + Duration::minutes(1));
                            if prev.is_none() {
                                print!(
                                    "===== {0} ONLINE ({1})\n",
                                    ip.to_string(),
                                    Local::now().format("%a %d %b %H:%M:%S"));
                            }
                            tx2.send(0).unwrap();
                        },
                        Err(_) => {}
                    }
                } 
            }
        });

        LogPrinter {
            hosts: HashMap::new(),
            last_host: std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            sender: tx,
            receiver: rx2,
        }
    }
}

impl MessageHandler<LoggerProto> for LogPrinter {
    fn on_message(&mut self, src: std::net::SocketAddr, msg: LoggerProto) -> anyhow::Result<()> {
        let date = Local::now();
        let curr_ts = msg.current_ts();
        let mut out = String::new();
        let mut new_line = true;
        let last_ts = self.hosts.entry(src).or_insert(0);
        if *last_ts > curr_ts {
            *last_ts = 0;
            maybe_print_header(&mut self.last_host, src.ip(), curr_ts);
            println!("------------>8------------");
        }
        self.sender.send(src.ip())?;
        self.receiver.recv()?;

        for record in &msg.record {
            let ts: u64 = record.ts();

            if ts < *last_ts {
                continue;
            }

            let dt = (curr_ts.checked_sub(ts)).unwrap_or(0).try_into().unwrap_or(0);
            let event_time = date - Duration::milliseconds(dt);

            for c in record.text().chars() {
                if new_line {
                    maybe_print_header(&mut self.last_host, src.ip(), curr_ts);
                    print!("{}", out);
                    out = String::new();
                    print!(
                        "{0}: {1}: ",
                        src.ip().to_string(),
                        event_time.format("%a %d %b %H:%M:%S")
                    );
                    new_line = false;
                }
                if c == '\n' || c == '\r' {
                    new_line = true;
                }
                out.push(c);
            }
        }
        print!("{}", out);

        if !new_line {
            println!("");
        }
        std::io::stdout().flush()?;
        *last_ts = curr_ts;
        Ok(())
    }
}


fn main() -> Result<()> {
    let mut log = LogPrinter::new();
    FragmentCombiner::new(&mut log).main_loop("192.168.0.1:6001")
}
