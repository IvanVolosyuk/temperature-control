use anyhow::Result;
use chrono::{DateTime, Local, TimeZone};
use clap::{Parser, Subcommand, ValueEnum};
use history::{
    read_and_parse_binary_state, read_and_parse_binary_state_v2, serialize_history_point,
    serialize_history_point_v2, ServerState,
};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Dump(DumpArgs),
    Convert(ConvertArgs),
}

#[derive(Parser)]
struct DumpArgs {
    #[arg(long)]
    file: String,
    #[arg(long)]
    format: Format,
}

#[derive(Parser)]
struct ConvertArgs {
    #[arg(long)]
    in_file: String,
    #[arg(long)]
    in_format: Format,
    #[arg(long)]
    out_file: String,
    #[arg(long)]
    out_format: Format,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum Format {
    V1,
    V2,
}

async fn read_state(file: &str, format: Format) -> Result<Option<(ServerState, Vec<u32>)>> {
    match format {
        Format::V1 => {
            let result = read_and_parse_binary_state(file).await?;
            Ok(result.map(|(state, ts0, ts2)| {
                let mut prev_timestamps = vec![0; 4];
                prev_timestamps[0] = ts0;
                prev_timestamps[2] = ts2;
                (state, prev_timestamps)
            }))
        }
        Format::V2 => {
            let result = read_and_parse_binary_state_v2(file).await?;
            Ok(result.map(|(state, ts)| (state, ts.to_vec())))
        }
    }
}

fn dump_state(server_state: &ServerState) {
    for room in &server_state.rooms {
        println!("Room: {} (ID: {})", room.name, room.id);
        for point in &room.temperature_history {
            let naive_datetime = Local.timestamp_opt(point.timestamp, 0).unwrap();
            let datetime: DateTime<Local> = DateTime::from(naive_datetime);
            println!(
                "  - {}: Temp: {:.1}, Target: {:.3}, Heater: {}, Disabled: {}",
                datetime.format("%Y-%m-%d %H:%M:%S"),
                point.temperature,
                point.target,
                if point.heater_on { "ON" } else { "OFF" },
                if point.is_disabled { "YES" } else { "NO" }
            );
        }
    }
}

async fn write_state(
    file: &str,
    format: Format,
    server_state: ServerState,
) -> Result<()> {
    let mut out_file = File::create(file).await?;
    let mut prev_timestamps = vec![0u32; 4];

    for room in server_state.rooms {
        for point in room.temperature_history {
            let device_id = room.id;
            let prev_ts = prev_timestamps[device_id as usize];
            let data = match format {
                Format::V1 => serialize_history_point(device_id, prev_ts, point.clone()),
                Format::V2 => serialize_history_point_v2(device_id, prev_ts, point.clone()),
            };
            for val in data {
                out_file.write_u32_le(val).await?;
            }
            prev_timestamps[device_id as usize] = point.timestamp as u32;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Dump(args) => {
            if let Some((server_state, _)) = read_state(&args.file, args.format).await? {
                dump_state(&server_state);
            } else {
                println!("No history data found in {}.", args.file);
            }
        }
        Commands::Convert(args) => {
            if let Some((server_state, _)) = read_state(&args.in_file, args.in_format).await? {
                write_state(&args.out_file, args.out_format, server_state).await?;
                println!(
                    "Successfully converted {} ({:?}) to {} ({:?}).",
                    args.in_file, args.in_format, args.out_file, args.out_format
                );
            } else {
                println!("No history data found in {}.", args.in_file);
            }
        }
    }

    Ok(())
}
