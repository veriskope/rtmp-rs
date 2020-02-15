use log::{info, trace, warn};
use tokio::prelude::*;
pub mod amf;
pub use amf::Value;

use crate::util;

// use rml_amf0::{Amf0Value};


mod signal;                         // declare module
pub use signal::Signal as Signal;   // export Signal as part of this module

// prefer that `signal` and `message` mods be invisible externally,
// will above syntax do that?
// alternate syntax
//use self::signal::Signal;
//use crate::chunk::signal::Signal;

mod message;
pub use message::Message as Message;
pub use message::Status as Status;

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
    let _timestamp:u32 = u32::from_be_bytes(ts);      // TODO: impl streams

    let length:u32 = u32::from_be_bytes([ 0x00, buf[3], buf[4], buf[5]]);

    let type_byte = reader.read_u8().await?;
    info!(target: "chunk", "message type: {}", type_byte);

    let message_stream_id = reader.read_u32().await?;  // TODO: impl streams
    info!(target: "chunk", "message stream id: {}", message_stream_id);

    // buffer for message payload
    let mut message_buf:Vec<u8> = vec![0; length as usize];

    let bytes_read = reader.read_exact(&mut message_buf)
                     .await.expect("read chunk message payload");
    if bytes_read < length as usize {
      warn!(target: "chunk", "bytes_read {} < length {}", bytes_read, length);
      panic!("expected {} bytes got {} bytes", length, bytes_read);
    }
    trace!(target: "chunk", "payload: {:02x?}", message_buf);
    let mut chunk_reader: &[u8] = &message_buf;

    let chunk: Chunk = match type_byte {
      1..=6 => Chunk::Control(Signal::read(&mut chunk_reader, type_byte).await?),
      20 => Chunk::Msg(Message::read(&mut chunk_reader, type_byte, length).await?),
      8..=22   => panic!("umimplemented RTMP message type: {}", type_byte),        // TODO: fail at some of these
      _  => panic!("unexpected chunk type: {}", type_byte),
    };
    Ok((chunk, header_size + length))
  }

  pub async fn write<T>(mut writer: T, chunk: Chunk) -> io::Result<u32>
  where T: AsyncWrite + Unpin
  {
    let _bytes_written:u32 = 0;
    match chunk {
      Chunk::Msg(message) => {
        // need to serialze the message first to get its length
        let mut buf = Vec::new();
        Message::write(&mut buf, message).await.expect("serialize message");
        // chunkstream message header
        let cs_id: u8 = 3;
        // TODO: handle diff chunk headers/msg types, just bootstrapping the process a bit
        // also the hack below handles lengths of just one byte
        let hex_str = format!("{:02x} 00 00 00 00 00 {:02x} 14  00 00 00 00", cs_id, buf.len());
        let chunk_header = util::bytes_from_hex_string(&hex_str);
        writer.write(&chunk_header).await.expect("write chunk header");
        writer.write(&buf).await.expect("write message");
      },
      Chunk::Control(s) => {
        panic!("unimplemented Chunk::Control {:?}", s)
      },
      // _ => {
      //   panic!("unimplemented chunk type: {:?}", chunk)
      // }
    }; // match chunk
    Ok(_bytes_written)
  } // fn write

} // impl Chunk

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;
    use amf::Value;
    use std::collections::HashMap;

  #[tokio::test]
  async fn can_write_connect_message() {
    // use crate::util::bytes_from_hex_string;
    // let cs_id: u8 = 3;
    //         let hex_str = format!("{:02x} 00 00 00 00 00 {:02x} 14  00 00 00 00", cs_id, message_length);

    // 03 00 00 00 00 00 6c 14  00 00 00 00
    // 02 00 07 63 6f 6e 6e 65 63 74 00 3f f0 00 00 00 00 00 00 03 00 03 61 70 70 02 00 09 76 6f 64 2f 6d 65 64 69 61 00 08 66 6c 61 73 68 56 65 72 02 00 0e 4d 41 43 20 31 30 2c 30 2c 33 32 2c 31 38 00 05 74 63 55 72 6c 02 00 1f 72 74 6d 70 3a 2f 2f 31 32 37 2e 30 2e 30 2e 31 3a 31 39 33 35 2f 76 6f 64 2f 6d 65 64 69 61 00 00 09");
    // let chunk_header = bytes_from_hex_string(&hex_str);

    let mut properties = HashMap::new();

    let app = "vod/media".to_string();
    properties.insert("app".to_string(), Value::Utf8(app));

    let flash_version = "MAC 10,0,32,18".to_string();   // TODO: must we, really?
    properties.insert("flashVer".to_string(), Value::Utf8(flash_version));

    properties.insert("objectEncoding".to_string(), Value::Number(0.0));

    let url = "rtmp://example.com/something".to_string();
    properties.insert("tcUrl".to_string(), Value::Utf8(url));

    let cmd = Message::Command {
              name: "connect".to_string(),
              id: 1.0,
              data: Value::Object(properties),     // shouldn't this be an array?
              opt: Value::Null };

    let mut buf = Vec::new();

    let _num_bytes = Chunk::write(&mut buf, Chunk::Msg(cmd)).await.expect("write");
    // test will fail if expect is triggered
    // assert_eq!(num_bytes, expected_bytes);

}

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
