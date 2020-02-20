use rtmp::util::bytes_from_hex_string;

fn main() {
  let hex_str = "01";

  let result = bytes_from_hex_string(hex_str);

  println!("one byte = {:?}", result);
}
