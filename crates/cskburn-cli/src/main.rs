use std::{borrow::Cow, fmt::Display, process::ExitCode, str::FromStr, time::Duration};

mod md5;
mod types;

use clap::{Args, Parser, Subcommand};
use console::Style;
use cskburn::{
    CSKBurn, ChipId, EraseTarget, Error, Family, Image, ProbeTarget, Region, ResetStrategy, Result,
    WriteTarget, list_ports,
};
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

    /// Chip family [possible values: venus, mars, arcs, venusa]
    #[arg(short = 'C', long)]
    chip: String,

    /// Path to burner image to use, omit to use built-in
    burner: Option<String>,

    /// Number of reset attempts during device probing
    #[arg(long, default_value_t = DEFAULT_RESET_ATTEMPTS)]
    reset_attempts: usize,

    /// Number of sync attempts per reset during probing
    #[arg(long, default_value_t = DEFAULT_SYNC_ATTEMPTS)]
    sync_attempts: usize,

    /// Reset pulse duration in milliseconds
    #[arg(long, default_value_t = DEFAULT_RESET_DELAY)]
    reset_delay: u64,

    /// Reset strategy describing how DTR/RTS map to BOOT/RESET.
    ///
    /// auto         — pick by chip; for ARCS alternates dtr-boot and dual-npn
    ///                across retries.
    /// dtr-boot     — DTR -> BOOT, RTS -> RESET (BOOT active low).
    ///                Typical: LS26 ARCS-MINI.
    /// rts-boot     — RTS -> BOOT, DTR -> RESET (BOOT active low).
    ///                Typical: CSK4 / CSK6 default.
    /// rts-boot-inv — same as rts-boot but BOOT is active high.
    /// dual-npn     — cross-coupled NPN pair (S8050).
    ///                Typical: LS26 ARCS-EVB.
    #[arg(long, default_value = "auto", value_parser = parse_reset_strategy_arg, verbatim_doc_comment)]
    reset_strategy: ResetStrategyArg,

    /// Disable progress bars and spinners
    #[arg(long)]
    no_progress: bool,

    #[command(subcommand)]
    command: Commands,
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

fn main() -> ExitCode {
    env_logger::init();

    let cli = Cli::parse();
    trace!("{:?}", cli);

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        // 128 + SIGINT — standard convention for user-cancelled processes.
        Err(Error::Cancelled) => ExitCode::from(130),
        Err(e) => {
            report_error(&e);
            // The numeric ErrorCode goes on stderr (parseable as [E####]).
            // Exit status stays a coarse 0/1/130 — POSIX only has 8 bits to
            // play with anyway, so packing 4-digit codes in there would
            // hash badly (3001 -> 185) and tell nobody anything useful.
            ExitCode::FAILURE
        }
    }
}

fn report_error(e: &Error) {
    let prefix = e
        .code()
        .map(|c| format!("[E{:04}]", c.as_u32()))
        .unwrap_or_else(|| "[E????]".to_string());
    eprintln!(
        "{} {}: {}",
        Style::new().red().bold().apply_to("ERROR"),
        prefix,
        e
    );
}

fn run(cli: Cli) -> Result<()> {
    let path = match cli.port {
        Some(p) => p,
        None => choose_port()?,
    };

    let chip = Family::from_str(&cli.chip)
        .map_err(|_| Error::ArgInvalid(format!("unsupported chip family: {}", cli.chip)))?;

    let mut burner = match cli.burner.as_deref() {
        None => chip.burner(),
        Some(p) => Image::try_from_file(0, p).map_err(|e| Error::FileReadFailed {
            path: p.to_string(),
            source: e,
        })?,
    };

    let mut cskburn = CSKBurn::connect(&path, cli.baud, chip)?;

    // Probe the device

    let progress = print_spinner("Probing", cli.no_progress);

    let reset_interval = Duration::from_millis(cli.reset_delay);

    let candidates: Vec<ResetStrategy> = match cli.reset_strategy {
        ResetStrategyArg::Auto => cskburn.reset_candidates().to_vec(),
        ResetStrategyArg::Fixed(s) => vec![s],
    };

    // Remember which strategy got us in, so the post-burn reset uses the same
    // wiring as the one that succeeded.
    let mut effective_strategy = candidates[0];

    let mut last_err: Option<Error> = None;
    for attempt in 0..cli.reset_attempts {
        let strategy = candidates[attempt % candidates.len()];
        effective_strategy = strategy;

        if attempt > 0 {
            progress.set_prefix(format!(
                "reset attempt {}/{} ({:?})",
                attempt + 1,
                cli.reset_attempts,
                strategy
            ));
        }

        cskburn.reset(Some(strategy), true, Some(reset_interval))?;

        match cskburn.probe(ProbeTarget::ROM, Some(cli.sync_attempts)) {
            Ok(()) => {
                last_err = None;
                break;
            }
            Err(e) if !e.is_retryable() => {
                progress.finish_and_clear();
                return Err(e);
            }
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        }
    }

    progress.finish_and_clear();
    if let Some(e) = last_err {
        // After exhausting retries, surface the last underlying failure so
        // the user sees the root cause (typically 4001 PROBE_NO_SYNC).
        return Err(e);
    }

    // Enter burner mode — write the burner image to RAM. RAM-write failures
    // here are presented to the user as "burner load failed" (5003), wrapping
    // the underlying 7005.

    let burner_size = burner.size().map_err(|e| Error::FileReadFailed {
        path: cli
            .burner
            .as_deref()
            .unwrap_or("<built-in burner>")
            .to_string(),
        source: e,
    })?;

    let progress = print_progress("Entering", burner_size, cli.no_progress);

    let mut load_err: Option<Error> = None;
    'load: {
        let iter = match cskburn.write_iter(&mut burner, WriteTarget::Memory { action: None }) {
            Ok(it) => it,
            Err(e) => {
                load_err = Some(e);
                break 'load;
            }
        };
        for step in iter {
            match step {
                Ok(s) => progress.set_position(s.bytes_written as u64),
                Err(e) => {
                    load_err = Some(e);
                    break 'load;
                }
            }
        }
    }
    if let Some(e) = load_err {
        progress.finish_and_clear();
        // RAM-write failures during burner load are user-facing 5003
        // (BurnerLoadFailed) with the underlying 7005 in the source chain.
        // Cancellation passes through unchanged.
        return Err(match e {
            Error::Cancelled => Error::Cancelled,
            other => Error::BurnerLoadFailed(Box::new(other)),
        });
    }

    cskburn.probe(ProbeTarget::Burner, Some(cli.sync_attempts))?;

    progress.finish_and_clear();

    // Read chip and flash info

    let chip_id = cskburn.chip_id()?;
    print_line("chip-id", format_chip_id(&chip_id, chip));

    let flash_id = cskburn.flash_info()?;
    let flash_size = flash_id.size() as u64;
    print_line(
        "flash-id",
        format!("{} ({} MiB)", flash_id, flash_size / 1024 / 1024),
    );

    // Run desired command

    match cli.command {
        Commands::Write(args) => {
            if args.erase_all {
                cskburn.flash_erase(EraseTarget::Entire)?;
            }

            type Md5Fn = Box<dyn Fn() -> Result<[u8; 16]>>;

            // Resolve all file specs into (label, image, region, md5_fn) tuples.
            // A single HEX file may produce multiple images.
            let mut images: Vec<(String, Image, Region, Md5Fn)> = Vec::new();

            for spec in &args.files {
                match spec {
                    FileSpec::Hex { path } => {
                        for hex_image in cskburn::hex::parse_hex(path, chip.base_addr())? {
                            let label = format!("{}@0x{:08x}", path, hex_image.addr);

                            let md5_fn: Md5Fn = Box::new({
                                let md5 = ::md5::compute(&hex_image.data).0;
                                move || Ok(md5)
                            });

                            let source = hex_image.into();
                            let region = make_region(&source)?;

                            images.push((label, source, region, md5_fn));
                        }
                    }
                    FileSpec::Raw { path, .. } => {
                        let label = format!("{}", spec);

                        let source =
                            Image::try_from(spec.clone()).map_err(|e| Error::FileReadFailed {
                                path: path.to_string(),
                                source: e,
                            })?;
                        let region = make_region(&source)?;

                        let spec_for_md5 = spec.clone();
                        let path_for_md5 = path.to_string();
                        let md5_fn: Md5Fn = Box::new(move || {
                            spec_for_md5.md5().map_err(|e| Error::VerifyLocalMd5Failed {
                                path: path_for_md5.clone(),
                                source: e,
                            })
                        });

                        images.push((label, source, region, md5_fn));
                    }
                }
            }

            for (i, (_, _, region, _)) in images.iter().enumerate() {
                let label = format!("partition {}", i + 1);
                validate_aligned(region.addr, FLASH_ALIGN, &label)?;
                validate_bounds(region.addr, region.size, flash_size, &label)?;
            }

            let count = images.len();
            for (i, (label, mut source, region, md5_fn)) in images.into_iter().enumerate() {
                print_line(format!("{}/{}", i + 1, count), label);

                if !chip.protocol().burner_supports_progressive_erase() {
                    let progress = print_spinner("Erasing", cli.no_progress);

                    cskburn.flash_erase(EraseTarget::Region(region.clone()))?;

                    progress.finish_and_clear();

                    print_step("Erased", format!("{}", region));
                }

                let progress = print_progress("Writing", region.size as usize, cli.no_progress);

                for step in cskburn.write_iter(&mut source, WriteTarget::Flash)? {
                    let step = step?;
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

                    let expected = md5_fn()?;
                    let actual = cskburn.flash_verify(region)?;

                    debug!("expect: {:02x?}", Md5(expected));
                    debug!("actual: {:02x?}", Md5(actual));

                    if expected != actual {
                        progress.finish_and_clear();
                        return Err(Error::VerifyMismatch { expected, actual });
                    }

                    progress.finish_and_clear();

                    print_step("Verified", format!("{}", Md5(actual)));
                }
            }

            print_step("Done", "".to_string());
        }
        Commands::Erase(args) => {
            for spec in &args.regions {
                let region: Region = spec.clone().into();
                validate_aligned(region.addr, FLASH_ALIGN, "erase region")?;
                validate_aligned(region.size, FLASH_ALIGN, "erase region size")?;
                validate_bounds(region.addr, region.size, flash_size, "erase region")?;
            }

            for spec in args.regions {
                cskburn.flash_erase(EraseTarget::Region(spec.into()))?;
            }
        }
        Commands::EraseAll => cskburn.flash_erase(EraseTarget::Entire)?,
        Commands::Verify(args) => {
            for spec in &args.regions {
                let region: Region = spec.clone().into();
                validate_bounds(region.addr, region.size, flash_size, "verify region")?;
            }

            for spec in args.regions {
                let md5 = cskburn.flash_verify(spec.into())?;
                println!("{:02x?}", md5);
            }
        }
    }

    // Reset the device to exit burner mode

    cskburn.reset(Some(effective_strategy), false, Some(reset_interval))?;

    Ok(())
}

fn make_region(image: &Image) -> Result<Region> {
    Region::try_from(image).map_err(|e| Error::FileReadFailed {
        path: "<image>".to_string(),
        source: e,
    })
}

fn choose_port() -> Result<String> {
    let ports: Vec<String> = list_ports()?;

    if ports.is_empty() {
        return Err(Error::SerialNotFound(
            "no USB serial ports found; pass --port explicitly".to_string(),
        ));
    }

    let choice = Select::new()
        .with_prompt("Choose a serial port")
        .default(0)
        .items(&ports)
        .interact()
        .map_err(|e| Error::ArgInvalid(format!("failed to read port choice: {}", e)))?;

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
        return Err(Error::ArgAddrUnaligned {
            addr: value,
            align,
            label: label.to_string(),
        });
    }
    Ok(())
}

fn validate_bounds(addr: u32, size: u32, flash_size: u64, label: &str) -> Result<()> {
    if (addr as u64) >= flash_size || (addr as u64) + (size as u64) > flash_size {
        return Err(Error::ArgAddrOutOfBounds {
            addr,
            size,
            capacity_mib: flash_size / 1024 / 1024,
            label: label.to_string(),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum ResetStrategyArg {
    Auto,
    Fixed(ResetStrategy),
}

fn parse_reset_strategy_arg(s: &str) -> std::result::Result<ResetStrategyArg, String> {
    if s == "auto" {
        return Ok(ResetStrategyArg::Auto);
    }
    ResetStrategy::from_str(s).map(ResetStrategyArg::Fixed).map_err(|_| {
        format!(
            "unknown reset strategy '{}', expected one of: auto, dtr-boot, rts-boot, rts-boot-inv, dual-npn",
            s
        )
    })
}

fn format_chip_id(chip_id: &ChipId, family: Family) -> String {
    match family {
        Family::ARCS => format!("{:x}", chip_id),
        _ => format!("{:X}", chip_id),
    }
}
