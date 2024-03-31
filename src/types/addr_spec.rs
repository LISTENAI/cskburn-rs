pub fn addr_from_str(s: &str) -> Result<u32, &'static str> {
    match if s.starts_with("0x") {
        u32::from_str_radix(&s[2..], 16)
    } else {
        s.parse()
    } {
        Ok(a) => Ok(a),
        Err(_) => Err("Invalid addr format"),
    }
}
