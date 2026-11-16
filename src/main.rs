mod battery;
mod iokit;
mod sampler;
mod smc;

use clap::{Parser, Subcommand};
use std::time::{Duration, Instant};

/// Sudoless power monitor for Apple Silicon laptops: adapter input, system draw and
/// battery charge/discharge, live and over time.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Update interval in milliseconds
    #[arg(short, long, default_value_t = 1000, global = true)]
    interval: u64,
}

#[derive(Subcommand)]
enum Command {
    /// Output samples as JSON lines (suitable for piping)
    Pipe {
        /// Number of samples to emit; 0 runs indefinitely
        #[arg(short, long, default_value_t = 0)]
        samples: u64,
    },
    /// Print raw sensor data: power-related SMC keys and the battery registry entry
    Debug {
        /// Only list SMC keys starting with this prefix (default: power-related keys)
        prefix: Option<String>,
    },
}

fn main() {
    // Exit quietly when the reader goes away (e.g. `macbat pipe | head`).
    unsafe extern "C" {
        fn signal(sig: i32, handler: usize) -> usize;
    }
    unsafe { signal(13 /* SIGPIPE */, 0 /* SIG_DFL */) };

    let cli = Cli::parse();
    let interval = Duration::from_millis(cli.interval.max(100));
    let result = match cli.command {
        None => pipe(interval, 0),
        Some(Command::Pipe { samples }) => pipe(interval, samples),
        Some(Command::Debug { prefix }) => debug(prefix),
    };
    if let Err(e) = result {
        eprintln!("macbat: {e}");
        std::process::exit(1);
    }
}

fn pipe(interval: Duration, samples: u64) -> Result<(), String> {
    let mut sampler = sampler::Sampler::new()?;
    let mut n = 0;
    loop {
        let started = Instant::now();
        let line = serde_json::to_string(&sampler.sample()).map_err(|e| e.to_string())?;
        println!("{line}");
        n += 1;
        if samples > 0 && n >= samples {
            return Ok(());
        }
        std::thread::sleep(interval.saturating_sub(started.elapsed()));
    }
}

fn debug(prefix: Option<String>) -> Result<(), String> {
    match smc::Smc::open() {
        Err(e) => println!("SMC: unavailable ({e})"),
        Ok(smc) => {
            let count = smc.key_count();
            println!("SMC: {count} keys");
            let wanted = |name: &str| match &prefix {
                Some(p) => name.starts_with(p.as_str()),
                None => ["PDTR", "PSTR", "VD0R", "ID0R", "PPBR", "B0AV", "B0AC", "TB0T", "TB1T", "TB2T"]
                    .contains(&name),
            };
            for i in 0..count {
                let Ok(key) = smc.key_at(i) else { continue };
                let name = smc::fourcc_str(key);
                if !wanted(&name) {
                    continue;
                }
                if let Ok((info, bytes)) = smc.read_raw(key) {
                    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                    let mut value = smc::decode(info.data_type, &bytes).map(|v| v.to_string()).unwrap_or_default();
                    // Integer keys may be little-endian on Apple Silicon; show both readings.
                    let ty = smc::fourcc_str(info.data_type);
                    if ty.starts_with("ui") || ty.starts_with("si") {
                        let mut le = bytes.clone();
                        le.reverse();
                        if let Some(v) = smc::decode(info.data_type, &le) {
                            value.push_str(&format!(" (le: {v})"));
                        }
                    }
                    println!("  {name}  {ty:<4}  {hex:<16}  {value}");
                }
            }
        }
    }
    println!();
    match battery::BatteryReader::open().and_then(|b| b.read()) {
        Ok(reg) => println!("Battery registry:\n{reg:#?}"),
        Err(e) => println!("Battery registry: unavailable ({e})"),
    }
    println!();
    let sample = sampler::Sampler::new()?.sample();
    println!("Sample:\n{}", serde_json::to_string_pretty(&sample).map_err(|e| e.to_string())?);
    Ok(())
}
