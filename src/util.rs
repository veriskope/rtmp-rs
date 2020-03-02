use log::trace;

//fn bytes_from_hex_string(hex_str: &str) -> &[u8] {
pub fn bytes_from_hex_string(hex_str: &str) -> Vec<u8> {
    trace!(target: "util::bytes_from_hex_string", "hex_str: {}", hex_str);
    let result = hex_str
        .split_whitespace()
        .map(|hex_digit| u8::from_str_radix(hex_digit, 16))
        .collect::<Result<Vec<u8>, _>>();
    match result {
        Ok(bytes) => {
            trace!(target: "util", "{:02X?}", bytes);
            return bytes;
        }
        Err(e) => panic!("error {} parsing: {}", e, hex_str),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_one() {
        assert_eq!(bytes_from_hex_string("01"), [0x1])
    }

    #[test]
    fn test_two() {
        assert_eq!(bytes_from_hex_string("01 0a"), [0x1, 0xa])
    }
} // mod tests
