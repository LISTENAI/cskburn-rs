mod cskburn;
mod types;

use clap::{Args, Parser, Subcommand, ValueEnum};
use dialoguer::Select;
use log::trace;
use serialport::available_ports;
use std::time::Duration;
use types::{FileSpec, RegionSpec};

/// Number of times to attempt to reset the device when probing before giving up.
/// In each attempt, a series of SYNC commands will be issued.
const PROBE_RESET_ATTEMPTS: usize = 2;

/// Number of times to attempt to issue a SYNC command when probing. Each attempt
/// takes about 500 ms.
const PROBE_SYNC_ATTEMPTS: usize = 3;

/// Interval between the reset pin being asserted and de-asserted.
const RESET_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Parser, Debug)]
#[command(version, author, about, long_about)]
struct Cli {
    /// Path to serial device
    #[arg(short, long)]
    port: Option<String>,

    /// Baud rate
    #[arg(short, long, default_value = "748800")]
    baud: u32,

    /// Chip family [possible values: 3, 4, 6]
    #[arg(short = 'C', long, default_value = "6")]
    chip: u8,

    /// Target to program
    #[arg(short, long, value_enum, default_value = "flash")]
    target: Target,

    #[command(subcommand)]
    command: Commands,
}

#[derive(ValueEnum, Clone, Debug)]
enum Target {
    FLASH,
    NAND,
    RAM,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Write files to flash
    Write(WriteArgs),

    /// Erase specific regions of flash
    Erase(EraseArgs),

    /// Erase entire flash
    EraseAll,

    /// Verify specific regions of flash
    Verify(VerifyArgs),
}

#[derive(Args, Debug)]
struct WriteArgs {
    /// Files to write, in the format ADDR:FILE
    ///
    /// ADDR - The offset to write the file to, can be in either decimal (e.g.
    ///        1048576) or hexadecimal (e.g. 0x100000).
    /// FILE - The path to the file to write.
    #[arg(value_name = "ADDR:FILE", required = true, verbatim_doc_comment)]
    files: Vec<FileSpec>,

    /// Erase entire flash before writing
    #[arg(long)]
    erase_all: bool,

    /// Verify all wrote regions after writing
    #[arg(long)]
    verify_all: bool,
}

#[derive(Args, Debug)]
struct EraseArgs {
    /// Regions to erase, in the format ADDR:SIZE
    ///
    /// ADDR - The offset to start erasing at, can be in either decimal (e.g.
    ///       1048576) or hexadecimal (e.g. 0x100000).
    /// SIZE - The size of the region to erase, can be in either decimal (e.g.
    ///       1048576) or hexadecimal (e.g. 0x100000).
    #[arg(value_name = "ADDR:SIZE", required = true, verbatim_doc_comment)]
    regions: Vec<RegionSpec>,
}

#[derive(Args, Debug)]
struct VerifyArgs {
    /// Regions to verify, in the format ADDR:SIZE
    ///
    /// ADDR - The offset to start erasing at, can be in either decimal (e.g.
    ///       1048576) or hexadecimal (e.g. 0x100000).
    /// SIZE - The size of the region to erase, can be in either decimal (e.g.
    ///       1048576) or hexadecimal (e.g. 0x100000).
    #[arg(value_name = "ADDR:SIZE", required = true, verbatim_doc_comment)]
    regions: Vec<RegionSpec>,
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();
    trace!("{:?}", cli);

    let path = if cli.port.is_none() {
        match choose_port() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("ERROR: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        cli.port.unwrap()
    };

    let mut cskburn = cskburn::new(path, cli.baud)
        .open()
        .expect("Failed to open device");

    if !cskburn.probe(None).is_ok() {
        let mut success = false;

        for _ in 0..PROBE_RESET_ATTEMPTS {
            cskburn
                .reset(true, Some(RESET_INTERVAL))
                .expect("Failed to reset device");

            if cskburn.probe(Some(PROBE_SYNC_ATTEMPTS)).is_ok() {
                success = true;
                break;
            }
        }

        if !success {
            panic!("Failed to detect device after multiple attempts");
        }
    }

    println!("Device detected");
}

fn choose_port() -> Result<String, &'static str> {
    let ports: Vec<String> = available_ports()
        .map_err(|_| "Failed to list serial ports")?
        .into_iter()
        .map(|p| p.port_name)
        .filter(|p| {
            if cfg!(target_os = "linux") {
                p.starts_with("/dev/ttyUSB") || p.starts_with("/dev/ttyACM")
            } else if cfg!(target_os = "macos") {
                p.starts_with("/dev/cu.usb")
            } else {
                true
            }
        })
        .collect();

    if ports.is_empty() {
        return Err("No serial ports found");
    }

    let choice = Select::new()
        .with_prompt("Choose a serial port")
        .default(0)
        .items(&ports)
        .interact()
        .map_err(|_| "Failed to get port choice")?;

    Ok(ports[choice].clone())
}
