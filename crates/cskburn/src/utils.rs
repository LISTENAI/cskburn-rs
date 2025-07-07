pub fn blocks(size: usize, block_size: usize) -> usize {
    (size + block_size - 1) / block_size
}

pub fn checksum(buf: &[u8]) -> u32 {
    let mut state = 0xef;
    for &byte in buf {
        state ^= byte as u32;
    }
    state
}
