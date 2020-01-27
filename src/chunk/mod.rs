use log::{info, warn};
use tokio::prelude::*;
pub mod amf;


mod signal;                         // declare module
pub use signal::Signal as Signal;   // export Signal as part of this module

// prefer that `signal` and `message` mods be invisible externally, 
// will above syntax do that?
// alternate syntax
//use self::signal::Signal;
//use crate::chunk::signal::Signal;

mod message;
pub use message::Message as Message;

// the table of constants could be merged with Enum declaration with
// https://github.com/rust-lang/rust/issues/60553

// #[derive(Debug)]
// enum Type {
//   // protocol control messages
//   SetChunkSize        = 01,
//   Abort               = 02,
//   AckChunk            = 03,
//   UserControlMessage  = 04,
//   SetWindowAckSize    = 05,
//   PeerBandwidth       = 06,

//   // RTMP message types
//   AudioMessage = 08,
//   VideoMessage = 09,
//   DataMessage3 = 15,
//   SharedObjectMessage3 = 16,
//   CommandMessage3 = 17,       // 0x11
//   DataMessage0 = 18,
//   SharedObjectMessage0 = 19,
//   CommandMessage0 = 20,       // 0x14   0xff  0123456789abcdef
//   AggregateMessage = 22,
// }

#[derive(Debug, PartialEq)]
pub enum Chunk {
  Control(Signal),
  Msg(Message)
}

impl Chunk {
  pub async fn read<T>(mut reader: T) -> io::Result<(Chunk, u32)>
  where T: AsyncRead + Unpin
  {
    let first_byte: u8;
    first_byte = reader.read_u8().await?;
  
    let fmt = first_byte >> 6;
    if fmt != 0 { warn!(target: "chunk", "Chunk Type {} unimplemented", fmt) };
    let header_size:u32 = 12;    // TODO: variable size chunkheader 
  
    let csid = first_byte & 0x3f;
    info!(target: "chunk", "csid: {}", csid);
  
    let mut buf: [u8; 6] = [0; 6];   // TODO: variable size chunkheader     
    reader.read_exact(&mut buf).await?;
  
    let ts =  [ 0x00, buf[0], buf[1], buf[2]];
    let _timestamp:u32 = u32::from_be_bytes(ts);      // TODO
  
    let length:u32 = u32::from_be_bytes([ 0x00, buf[3], buf[4], buf[5]]);
  
    let type_byte = reader.read_u8().await?;
    info!(target: "chunk", "type_byte: {}", type_byte);
  
    let _message_stream_id = reader.read_u32().await?;  // TODO
  
    let chunk: Chunk = match type_byte {
      1..=6 => Chunk::Control(Signal::read(reader, type_byte).await?),
      20 => Chunk::Msg(Message::read(reader, type_byte, length).await?),
      8..=22   => panic!("umimplemented RTMP message type: {}", type_byte),        // TODO: fail at some of these
      _  => panic!("unexpected chunk type: {}", type_byte),
    };
    Ok((chunk, header_size + length))
  }
}


#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

#[tokio::test]
async fn can_read_chunk_set_window_ack_size() {
  use crate::util::bytes_from_hex_string;
//  let bytes = bytes_from_hex_string("02 00 00 00 00 00 05 06 00 00 00 00 00 26 25 a0 02");
  let bytes = bytes_from_hex_string("02 00 00 00 00 00 04 05 00 00 00 00 00 26 25 a0");

  let buf: &[u8] = &bytes;
  let (chunk, num_bytes) = Chunk::read(buf).await.expect("read");
  assert_eq!(chunk, Chunk::Control(Signal::SetWindowAckSize(2500000)));
  assert_eq!(num_bytes, 16);

}

#[tokio::test]
async fn can_read_chunk_set_peer_bandwidth() {
  use crate::util::bytes_from_hex_string;
 let bytes = bytes_from_hex_string("02 00 00 00 00 00 05 06 00 00 00 00 00 26 25 a0 02");

  let buf: &[u8] = &bytes;
  let (chunk, num_bytes) = Chunk::read(buf).await.expect("read");
  assert_eq!(chunk, Chunk::Control(Signal::SetPeerBandwidth(2500000, 2)));
  assert_eq!(num_bytes, 17);
}


// #[tokio::test]
// async fn can_read_command_message() {
//   use crate::util::bytes_from_hex_string;
//  let bytes = bytes_from_hex_string(
//   "03 00 00 00 00 00 f4 14  00 00 00 00 
//   02 00 07 5f 72 65 73 75  6c 74 00 3f f0 00 00 00  
//   00 00 00 03 00 06 66 6d  73 56 65 72 02 00 0f 46  
//   4d 53 2f 35 2c 30 2c 31  35 2c 35 30 30 34 00 0c  
//   63 61 70 61 62 69 6c 69  74 69 65 73 00 40 6f e0  
//   00 00 00 00 00 00 04 6d  6f 64 65 00 3f f0 00 00  
//   00 00 00 00 00 00 09 03  00 05 6c 65 76 65 6c 02  
//   00 06 73 74 61 74 75 73  00 04 63 6f 64 65 02 00  
//   1d 4e 65 74 43 6f 6e 6e  65 63 74 69 6f 6e 2e 43  
//   6f 6e 6e 65 63 74 2e 53  75 63 63 65 73 73 00 0b  
//   64 65 73 63 72 69 70 74  69 6f 6e 02 00 15 43 6f  
//   6e 6e 65 63 74 69 6f 6e  20 73 75 63 63 65 65 64  
//   65 64 2e 00 0e 6f 62 6a  65 63 74 45 6e 63 6f 64  
//   69 6e 67 00 00 00 00 00  00 00 00 00 00 04 64 61  
//   74 61 08 00 00 00 00 00  07 76 65 72 73 69 6f 6e  
//   02 00 0b 35 2c 30 2c 31  35 2c 35 30 30 34 00 00  
//   09 00 00 09");

//   let buf: &[u8] = &bytes;
//   let (chunk, num_bytes) = Chunk::read(buf).await.expect("read");
//   // TODO
//   // let value = Value::Utf8String("test".to_string());
//   // // assert_eq!(chunk, Chunk::Msg(Message::Command(value)));
// }






} // mod tests
