use std::{borrow::Cow, fmt::Display, str::FromStr, time::Duration};

mod md5;
mod types;

use anyhow::{Result, anyhow};
use clap::{Args, Parser, Subcommand, ValueEnum};
use console::Style;
use cskburn::{EraseTarget, Family, Image, ProbeTarget, Region, list_ports};
use dialoguer::Select;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, trace};

use crate::md5::Md5;
use crate::types::{FileSpec, RegionSpec};

/// Number of times to attempt to reset the device when probing before giving up.
/// In each attempt, a series of SYNC commands will be issued.
const PROBE_RESET_ATTEMPTS: usize = 5;

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
    #[arg(short, long, default_value = "1500000")]
    baud: u32,

    /// Chip family [possible values: 3, 4, 6]
    #[arg(short = 'C', long, default_value = "6")]
    chip: String,

    /// Path to burner image to use, omit to use built-in
    burner: Option<String>,

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

fn main() -> Result<()> {
    env_logger::init();

    // Parse command line arguments

    let cli = Cli::parse();
    trace!("{:?}", cli);

    let path = cli.port.map(Ok).unwrap_or_else(|| choose_port())?;

    let chip =
        Family::from_str(&cli.chip).map_err(|_| anyhow!("Invalid chip family: {}", cli.chip))?;

    let mut burner = cli.burner.map_or_else(
        || Ok(chip.burner()),
        |path| {
            Image::try_from_file(0, &path)
                .map_err(|e| anyhow!("Failed to open burner image file: {}", e))
        },
    )?;

    // Create and open the device

    let mut cskburn = cskburn::new(path, cli.baud, chip)
        .open()
        .map_err(|e| anyhow!("Failed to open device: {}", e))?;

    // Probe the device

    let progress = print_spinner("Probing");

    if !cskburn.probe(ProbeTarget::ROM, None).is_ok() {
        let mut success = false;

        for attempts in 0..PROBE_RESET_ATTEMPTS {
            if attempts > 0 {
                progress.set_prefix(format!(
                    "reset attempt {}/{}",
                    attempts + 1,
                    PROBE_RESET_ATTEMPTS
                ));
            }

            cskburn
                .reset(true, Some(RESET_INTERVAL))
                .map_err(|e| anyhow!("Failed to reset device: {}", e))?;

            if cskburn
                .probe(ProbeTarget::ROM, Some(PROBE_SYNC_ATTEMPTS))
                .is_ok()
            {
                success = true;
                break;
            }
        }

        if !success {
            progress.finish_and_clear();
            return Err(anyhow!("Failed to detect device after multiple attempts"));
        }
    }

    progress.finish_and_clear();

    // Enter burner mode

    let burner_size = burner
        .size()
        .map_err(|e| anyhow!("Failed to get burner size: {}", e))?;

    let progress = print_progress("Entering", burner_size);

    cskburn
        .memory_write(
            &mut burner,
            None,
            Some(&mut |written: usize, _: usize| progress.set_position(written as u64)),
        )
        .map_err(|e| anyhow!("Failed to write burner image: {}", e))?;

    cskburn
        .probe(ProbeTarget::Burner, Some(PROBE_SYNC_ATTEMPTS))
        .map_err(|e| anyhow!("Failed entering burner mode: {}", e))?;

    progress.finish_and_clear();

    // Read chip and flash info

    let chip_id = cskburn
        .chip_id()
        .map_err(|e| anyhow!("Failed to read chip ID: {}", e))?;
    print_line("chip-id", format!("{}", chip_id));

    let flash_id = cskburn
        .flash_info()
        .map_err(|e| anyhow!("Failed to read flash ID: {}", e))?;
    print_line(
        "flash-id",
        format!("{} ({} MiB)", flash_id, flash_id.size() / 1024 / 1024),
    );

    // Run desired command

    match cli.command {
        Commands::Write(args) => {
            if args.erase_all {
                cskburn
                    .flash_erase(EraseTarget::Entire)
                    .map_err(|e| anyhow!("Failed to erase flash: {}", e))?;
            }

            let files = args
                .files
                .into_iter()
                .map(|spec| {
                    let source = Image::try_from(spec.clone())
                        .map_err(|e| anyhow!("Failed to open file: {}", e))?;
                    let region = Region::try_from(&source)
                        .map_err(|e| anyhow!("Failed to create region from file: {}", e))?;
                    Ok((spec, source, region))
                })
                .collect::<Result<Vec<(FileSpec, Image, Region)>>>()?;

            let count = files.len();
            for (i, (file, mut source, region)) in files.into_iter().enumerate() {
                print_line(format!("{}/{}", i + 1, count), format!("{}", file));

                let progress = print_progress("Writing", region.size as usize);

                cskburn
                    .flash_write(
                        &mut source,
                        Some(&mut |written: usize, _: usize| progress.set_position(written as u64)),
                    )
                    .map_err(|e| anyhow!("Failed to write file: {}", e))?;

                progress.finish_and_clear();

                print_step(
                    "Wrote",
                    format!(
                        "{} bytes, took {}s",
                        region.size,
                        progress.elapsed().as_secs()
                    ),
                );

                if args.verify_all {
                    let progress = print_spinner("Verifying");

                    let expect_md5 = Md5(file
                        .md5()
                        .map_err(|e| anyhow!("Failed to calculate file MD5: {}", e))?);
                    let actual_md5 = Md5(cskburn
                        .flash_verify(region)
                        .map_err(|e| anyhow!("Failed to verify file: {}", e))?);

                    debug!("expect: {:02x?}", expect_md5);
                    debug!("actual: {:02x?}", actual_md5);

                    if expect_md5 != actual_md5 {
                        progress.finish_and_clear();
                        return Err(anyhow!(
                            "File verification failed: expected {:02x?}, got {:02x?}",
                            expect_md5,
                            actual_md5
                        ));
                    }

                    progress.finish_and_clear();

                    print_step("Verified", format!("{}", actual_md5));
                }
            }

            print_step("Done", "".to_string());
        }
        Commands::Erase(args) => {
            for spec in args.regions {
                cskburn
                    .flash_erase(EraseTarget::Region(spec.into()))
                    .map_err(|e| anyhow!("Failed to erase flash region: {}", e))?;
            }
        }
        Commands::EraseAll => cskburn
            .flash_erase(EraseTarget::Entire)
            .map_err(|e| anyhow!("Failed to erase flash: {}", e))?,
        Commands::Verify(args) => {
            for spec in args.regions {
                let md5 = cskburn
                    .flash_verify(spec.into())
                    .map_err(|e| anyhow!("Failed to verify flash region: {}", e))?;
                println!("{:02x?}", md5);
            }
        }
    }

    // Reset the device to exit burner mode

    cskburn
        .reset(false, Some(RESET_INTERVAL))
        .map_err(|e| anyhow!("Failed to reset device: {}", e))?;

    Ok(())
}

fn choose_port() -> Result<String> {
    let ports: Vec<String> =
        list_ports().map_err(|e| anyhow!("Failed to list serial ports: {}", e))?;

    if ports.is_empty() {
        return Err(anyhow!("No serial ports found"));
    }

    let choice = Select::new()
        .with_prompt("Choose a serial port")
        .default(0)
        .items(&ports)
        .interact()
        .map_err(|e| anyhow!("Failed to get port choice: {}", e))?;

    Ok(ports[choice].clone())
}

fn print_spinner<T>(prefix: T) -> ProgressBar
where
    T: Into<Cow<'static, str>>,
{
    let style = ProgressStyle::with_template("{msg:>12.cyan.bold} {spinner} {prefix}").unwrap();
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(style);
    spinner.set_message(prefix);
    spinner.enable_steady_tick(Duration::from_millis(100));
    spinner
}

fn print_progress<T>(prefix: T, len: usize) -> ProgressBar
where
    T: Into<Cow<'static, str>>,
{
    let style = ProgressStyle::with_template("{msg:>12.cyan.bold} [{bar:20}] {binary_bytes}/{binary_total_bytes} @ {bytes_per_sec} (eta {eta})")
            .unwrap()
            .progress_chars("=> ");
    let progress = ProgressBar::new(len as u64);
    progress.set_style(style);
    progress.set_message(prefix);
    progress
}

fn print_step<T>(prefix: T, message: String)
where
    T: Into<Cow<'static, str>> + Display,
{
    println!(
        "{:>12} {}",
        Style::new().green().bold().apply_to(prefix),
        message
    )
}

fn print_line<T>(prefix: T, message: String)
where
    T: Into<Cow<'static, str>> + Display,
{
    println!("{:>12} {}", prefix, message)
}
