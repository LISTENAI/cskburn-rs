use crate::family::{ChipProtocol, ResetStrategy};

pub struct MarsProtocol;

impl ChipProtocol for MarsProtocol {
    fn reset_candidates(&self) -> &'static [ResetStrategy] {
        &[ResetStrategy::DualNpn]
    }

    fn rom_requires_adaptive_duplex(&self) -> bool {
        true
    }
}
