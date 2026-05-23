use std::time::Duration;

use serialport::SerialPort;

use crate::family::{ChipProtocol, ResetStrategy};

pub struct ArcsProtocol;

impl ChipProtocol for ArcsProtocol {
    fn reset(
        &self,
        port: &mut dyn SerialPort,
        strategy: ResetStrategy,
        boot_mode: bool,
        reset_interval: Option<Duration>,
    ) -> Result<(), std::io::Error> {
        // ARCS needs a longer pulse than the 100ms default; the original
        // sequence used 500ms minimum, preserve that floor here.
        let reset_delay = reset_interval
            .unwrap_or_default()
            .max(Duration::from_millis(500));
        strategy.apply(port, boot_mode, reset_delay)
    }

    fn reset_candidates(&self) -> &'static [ResetStrategy] {
        // Auto-mode alternates between the two known ARCS production boards:
        // ARCS-MINI uses direct pin wiring, ARCS-EVB uses the cross-coupled
        // NPN pair.
        &[ResetStrategy::DtrBoot, ResetStrategy::DualNpn]
    }

    fn burner_supports_progressive_erase(&self) -> bool {
        false
    }
}
