use crate::family::ChipProtocol;

pub struct VenusaProtocol;

impl ChipProtocol for VenusaProtocol {
    fn burner_supports_progressive_erase(&self) -> bool {
        false
    }
}
