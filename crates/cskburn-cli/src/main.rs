use std::{borrow::Cow, fmt::Display, str::FromStr, time::Duration};

mod hex;
mod md5;
mod types;

use anyhow::{Result, anyhow};
use clap::{Args, Parser, Subcommand, ValueEnum};
use console::Style;
use cskburn::{CSKBurn, EraseTarget, Family, Image, ProbeTarget, Region, WriteTarget, list_ports};
use dialoguer::Select;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, trace};

use crate::md5::Md5;
use crate::types::{FileSpec, RegionSpec};

const DEFAULT_RESET_ATTEMPTS: usize = 5;
const DEFAULT_SYNC_ATTEMPTS: usize = 3;
const DEFAULT_RESET_DELAY: u64 = 100;

const FLASH_ALIGN: u32 = 4096;

#[derive(Parser, Debug)]
#[command(version, author, about, long_about)]
struct Cli {
    /// Path to serial device
    #[arg(short, long)]
    port: Option<String>,

    /// Baud rate
    #[arg(short, long, default_value = "1500000")]
    baud: u32,

    /// Chip family [possible values: venus, mars, arcs]
    #[arg(short = 'C', long)]
    chip: String,

    /// Path to burner image to use, omit to use built-in
    burner: Option<String>,

    /// Target to program
    #[arg(short, long, value_enum, default_value = "flash")]
    target: Target,

    /// Number of reset attempts during device probing
    #[arg(long, default_value_t = DEFAULT_RESET_ATTEMPTS)]
    reset_attempts: usize,

    /// Number of sync attempts per reset during probing
    #[arg(long, default_value_t = DEFAULT_SYNC_ATTEMPTS)]
    sync_attempts: usize,

    /// Reset pulse duration in milliseconds
    #[arg(long, default_value_t = DEFAULT_RESET_DELAY)]
    reset_delay: u64,

    /// Disable progress bars and spinners
    #[arg(long)]
    no_progress: bool,

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

    let chip = Family::from_str(&cli.chip)
        .map_err(|_| anyhow!("Unsupported chip family: {}", cli.chip))?;

    let mut burner = cli.burner.map_or_else(
        || Ok(chip.burner()),
        |path| {
            Image::try_from_file(0, &path)
                .map_err(|e| anyhow!("Failed to open burner image file: {}", e))
        },
    )?;

    // Create and open the device

    let mut cskburn = CSKBurn::connect(&path, cli.baud, chip)
        .map_err(|e| anyhow!("Failed to open device: {}", e))?;

    // Probe the device

    let progress = print_spinner("Probing", cli.no_progress);

    let reset_interval = Duration::from_millis(cli.reset_delay);

    let mut success = false;
    for attempts in 0..cli.reset_attempts {
        if attempts > 0 {
            progress.set_prefix(format!(
                "reset attempt {}/{}",
                attempts + 1,
                cli.reset_attempts
            ));
        }

        cskburn
            .reset(true, Some(reset_interval))
            .map_err(|e| anyhow!("Failed to reset device: {}", e))?;

        if cskburn
            .probe(ProbeTarget::ROM, Some(cli.sync_attempts))
            .is_ok()
        {
            success = true;
            break;
        }
    }

    progress.finish_and_clear();
    if !success {
        return Err(anyhow!("Failed to detect device after multiple attempts"));
    }

    // Enter burner mode

    let burner_size = burner
        .size()
        .map_err(|e| anyhow!("Failed to get burner size: {}", e))?;

    let progress = print_progress("Entering", burner_size, cli.no_progress);

    for step in cskburn
        .write_iter(&mut burner, WriteTarget::Memory { action: None })
        .map_err(|e| anyhow!("Failed to write burner image: {}", e))?
    {
        let step = step.map_err(|e| anyhow!("Failed to write burner image: {}", e))?;
        progress.set_position(step.bytes_written as u64);
    }

    cskburn
        .probe(ProbeTarget::Burner, Some(cli.sync_attempts))
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
    let flash_size = flash_id.size() as u64;
    print_line(
        "flash-id",
        format!("{} ({} MiB)", flash_id, flash_size / 1024 / 1024),
    );

    // Run desired command

    match cli.command {
        Commands::Write(args) => {
            if args.erase_all {
                cskburn
                    .flash_erase(EraseTarget::Entire)
                    .map_err(|e| anyhow!("Failed to erase flash: {}", e))?;
            }

            type Md5Fn = Box<dyn Fn() -> Result<[u8; 16]>>;

            // Resolve all file specs into (label, image, region, md5_fn) tuples.
            // A single HEX file may produce multiple images.
            let mut images: Vec<(String, Image, Region, Md5Fn)> = Vec::new();

            for spec in &args.files {
                match spec {
                    FileSpec::Hex { path } => {
                        for hex_image in hex::parse_hex(path, chip.base_addr())? {
                            let label = format!("{}@0x{:08x}", path, hex_image.addr);

                            let md5_fn: Md5Fn = Box::new({
                                let hex_image = &hex_image;
                                let md5 = hex_image.md5();
                                move || Ok(md5)
                            });

                            let source = hex_image.into();
                            let region = Region::try_from(&source)
                                .map_err(|e| anyhow!("Failed to create region: {}", e))?;

                            images.push((label, source, region, md5_fn));
                        }
                    }
                    FileSpec::Raw { .. } => {
                        let label = format!("{}", spec);

                        let source = Image::try_from(spec.clone())
                            .map_err(|e| anyhow!("Failed to open file: {}", e))?;
                        let region = Region::try_from(&source)
                            .map_err(|e| anyhow!("Failed to create region: {}", e))?;

                        let spec = spec.clone();
                        let md5_fn: Md5Fn = Box::new(move || {
                            spec.md5()
                                .map_err(|e| anyhow!("Failed to calculate file MD5: {}", e))
                        });

                        images.push((label, source, region, md5_fn));
                    }
                }
            }

            for (i, (_, _, region, _)) in images.iter().enumerate() {
                validate_aligned(region.addr, FLASH_ALIGN, &format!("partition {}", i + 1))?;
                validate_bounds(
                    region.addr,
                    region.size,
                    flash_size,
                    &format!("partition {}", i + 1),
                )?;
            }

            let count = images.len();
            for (i, (label, mut source, region, md5_fn)) in images.into_iter().enumerate() {
                print_line(format!("{}/{}", i + 1, count), label);

                if !chip.protocol().burner_supports_progressive_erase() {
                    let progress = print_spinner("Erasing", cli.no_progress);

                    cskburn
                        .flash_erase(EraseTarget::Region(region.clone()))
                        .map_err(|e| anyhow!("Failed to erase flash region: {}", e))?;

                    progress.finish_and_clear();

                    print_step("Erased", format!("{}", region));
                }

                let progress = print_progress("Writing", region.size as usize, cli.no_progress);

                for step in cskburn
                    .write_iter(&mut source, WriteTarget::Flash)
                    .map_err(|e| anyhow!("Failed to write file: {}", e))?
                {
                    let step = step.map_err(|e| anyhow!("Failed to write file: {}", e))?;
                    progress.set_position(step.bytes_written as u64);
                }

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
                    let progress = print_spinner("Verifying", cli.no_progress);

                    let expect_md5 = Md5(md5_fn()?);
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
            for spec in &args.regions {
                let region: Region = spec.clone().into();
                validate_aligned(region.addr, FLASH_ALIGN, "erase address")?;
                validate_aligned(region.size, FLASH_ALIGN, "erase size")?;
                validate_bounds(region.addr, region.size, flash_size, "erase region")?;
            }

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
            for spec in &args.regions {
                let region: Region = spec.clone().into();
                validate_bounds(region.addr, region.size, flash_size, "verify region")?;
            }

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
        .reset(false, Some(reset_interval))
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

fn print_spinner<T>(prefix: T, hidden: bool) -> ProgressBar
where
    T: Into<Cow<'static, str>>,
{
    if hidden {
        return ProgressBar::hidden();
    }
    let style = ProgressStyle::with_template("{msg:>12.cyan.bold} {spinner} {prefix}").unwrap();
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(style);
    spinner.set_message(prefix);
    spinner.enable_steady_tick(Duration::from_millis(100));
    spinner
}

fn print_progress<T>(prefix: T, len: usize, hidden: bool) -> ProgressBar
where
    T: Into<Cow<'static, str>>,
{
    if hidden {
        return ProgressBar::hidden();
    }
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

fn validate_aligned(value: u32, align: u32, label: &str) -> Result<()> {
    if value % align != 0 {
        return Err(anyhow!(
            "Address 0x{:08x} of {} should be {} aligned",
            value,
            label,
            format_size(align as u64),
        ));
    }
    Ok(())
}

fn validate_bounds(addr: u32, size: u32, flash_size: u64, label: &str) -> Result<()> {
    if (addr as u64) >= flash_size {
        return Err(anyhow!(
            "Start address 0x{:08x} of {} exceeds flash capacity ({} MiB)",
            addr,
            label,
            flash_size / 1024 / 1024,
        ));
    }
    if (addr as u64) + (size as u64) > flash_size {
        return Err(anyhow!(
            "End address 0x{:08x} of {} exceeds flash capacity ({} MiB)",
            addr + size,
            label,
            flash_size / 1024 / 1024,
        ));
    }
    Ok(())
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{}M", bytes / 1024 / 1024)
    } else if bytes >= 1024 {
        format!("{}K", bytes / 1024)
    } else {
        format!("{}B", bytes)
    }
}
