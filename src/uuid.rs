//! Random ids in UUID form, as Deezer's listen reports and AirPlay sessions take
//! them.

/// A random RFC 4122 version 4 UUID, lowercase: "1b4e28ba-2fa1-41d2-883f-0016d3cca427".
pub fn v4() -> String {
    let mut u = [0u8; 16];
    getrandom::fill(&mut u).expect("OS RNG");
    u[6] = (u[6] & 0x0f) | 0x40;
    u[8] = (u[8] & 0x3f) | 0x80;
    let hex: String = u.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_4_uuids() {
        let id = super::v4();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.iter().map(|p| p.len()).collect::<Vec<_>>(), [8, 4, 4, 4, 12], "{id}");
        assert!(parts[2].starts_with('4'), "version 4: {id}");
        assert!(matches!(parts[3].as_bytes()[0], b'8' | b'9' | b'a' | b'b'), "RFC 4122 variant: {id}");
        assert!(id.chars().all(|c| c == '-' || c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(id, super::v4());
    }
}
