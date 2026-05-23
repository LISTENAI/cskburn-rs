use std::{thread::sleep, time::Duration};

use log::trace;
use serialport::SerialPort;

/// Reset circuit topology — describes how DTR and RTS are wired to the chip's
/// BOOT and RESET pins. Each variant produces a different pulse sequence on
/// the control lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetStrategy {
    /// DTR drives BOOT, RTS drives RESET. BOOT is active low.
    /// Typical: LS26 ARCS-MINI development board.
    DtrBoot,
    /// RTS drives BOOT, DTR drives RESET. BOOT is active low.
    /// Typical: CSK4 / CSK6 default wiring.
    RtsBoot,
    /// Same wiring as [`RtsBoot`] but BOOT is active high.
    /// Equivalent to the legacy `--update-high` flag.
    RtsBootInverted,
    /// Two NPN transistors (S8050) with cross-coupled base/emitter, where
    /// DTR>RTS pulls PRST low (Q1) and RTS>DTR pulls RXD low (Q2).
    /// Typical: LS26 ARCS-EVB development board.
    DualNpn,
}

impl ResetStrategy {
    pub fn apply(
        &self,
        port: &mut dyn SerialPort,
        boot_mode: bool,
        reset_delay: Duration,
    ) -> std::io::Result<()> {
        match self {
            Self::DtrBoot => dtr_boot(port, boot_mode, reset_delay),
            Self::RtsBoot => rts_boot(port, boot_mode, reset_delay, false),
            Self::RtsBootInverted => rts_boot(port, boot_mode, reset_delay, true),
            Self::DualNpn => dual_npn(port, boot_mode, reset_delay),
        }
    }
}

impl std::str::FromStr for ResetStrategy {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dtr-boot" => Ok(Self::DtrBoot),
            "rts-boot" => Ok(Self::RtsBoot),
            "rts-boot-inv" => Ok(Self::RtsBootInverted),
            "dual-npn" => Ok(Self::DualNpn),
            _ => Err("unknown reset strategy"),
        }
    }
}

// Mapping convention between this code and the underlying serial line:
//
//   port.write_request_to_send(true)       -> RTS asserted (line driven LOW)
//   port.write_data_terminal_ready(true)   -> DTR asserted (line driven LOW)
//
// The comments below describe the *pin state on the chip*, which depends on
// the wiring of each strategy.

fn dtr_boot(
    port: &mut dyn SerialPort,
    boot_mode: bool,
    reset_delay: Duration,
) -> std::io::Result<()> {
    // Hold RESET first so any BOOT toggle can't run user code.
    trace!("dtr-boot: RESET=low, BOOT=released");
    port.write_request_to_send(true)?; // RESET=low
    port.write_data_terminal_ready(false)?; // BOOT released (active low)

    if boot_mode {
        sleep(Duration::from_millis(50));
        trace!("dtr-boot: assert BOOT while RESET still low");
        port.write_data_terminal_ready(true)?; // BOOT asserted (low)
    }

    sleep(reset_delay);

    trace!("dtr-boot: release RESET");
    port.write_request_to_send(false)?; // RESET released

    if boot_mode {
        sleep(Duration::from_millis(50));
    }

    trace!("dtr-boot: release BOOT");
    port.write_data_terminal_ready(false)?; // BOOT released

    Ok(())
}

fn rts_boot(
    port: &mut dyn SerialPort,
    boot_mode: bool,
    reset_delay: Duration,
    boot_active_high: bool,
) -> std::io::Result<()> {
    // For an active-low BOOT pin, "boot asserted" means RTS driven low, i.e.
    // write_request_to_send(true). For active-high BOOT, the polarity flips.
    let assert_boot = !boot_active_high;

    trace!(
        "rts-boot{}: release BOOT",
        if boot_active_high { "-inv" } else { "" }
    );
    port.write_request_to_send(!assert_boot)?; // BOOT released

    if boot_mode {
        port.write_data_terminal_ready(false)?; // RESET released
        sleep(Duration::from_millis(10));
        trace!("rts-boot: assert BOOT");
        port.write_request_to_send(assert_boot)?; // BOOT asserted
    }

    trace!("rts-boot: assert RESET");
    port.write_data_terminal_ready(true)?; // RESET asserted

    sleep(reset_delay);

    trace!("rts-boot: release RESET");
    port.write_data_terminal_ready(false)?; // RESET released

    Ok(())
}

fn dual_npn(
    port: &mut dyn SerialPort,
    boot_mode: bool,
    reset_delay: Duration,
) -> std::io::Result<()> {
    // Q1 conducts when DTR > RTS (DTR high, RTS low), pulling PRST low.
    // Q2 conducts when RTS > DTR (RTS high, DTR low), pulling RXD low.
    trace!("dual-npn: assert PRST via Q1");
    port.write_data_terminal_ready(false)?; // DTR high (TIA-232 high)
    port.write_request_to_send(true)?; // RTS low

    sleep(reset_delay);

    if boot_mode {
        trace!("dual-npn: release PRST and pull RXD low via Q2");
        port.write_data_terminal_ready(true)?; // DTR low
        port.write_request_to_send(false)?; // RTS high
        sleep(Duration::from_millis(50));
    }

    trace!("dual-npn: release both, UART free");
    port.write_data_terminal_ready(true)?; // DTR low
    port.write_request_to_send(true)?; // RTS low

    Ok(())
}
