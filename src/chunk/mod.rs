use crate::connection::bufreadwriter::BufReadWriter;
use log::{info, trace, warn};
use std::convert::TryInto;
use tokio::prelude::*;
// use rml_amf0::{Amf0Value};

mod signal; // declare module
pub use signal::Signal; // export Signal as part of this module

// prefer that `signal` and `message` mods be invisible externally,
// will above syntax do that?
// alternate syntax
//use self::signal::Signal;
//use crate::chunk::signal::Signal;

use crate::message::*;

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
    Msg(Message),
}

impl Chunk {
    pub async fn read<T>(mut reader: T) -> io::Result<(Chunk, u32)>
    where
        T: AsyncRead + Unpin,
    {
        let first_byte: u8;
        first_byte = reader.read_u8().await?;

        let fmt = first_byte >> 6;
        if fmt != 0 {
            warn!(target: "chunk::read", "Chunk Type {} unimplemented", fmt)
        };
        let header_size: u32 = 12; // TODO: variable size chunk header

        let csid = first_byte & 0x3f;
        info!(target: "chunk::read", "csid: {}", csid);

        let mut buf: [u8; 6] = [0; 6]; // TODO: variable size chunk header
        reader.read_exact(&mut buf).await?;

        let ts = [0x00, buf[0], buf[1], buf[2]];
        let _timestamp: u32 = u32::from_be_bytes(ts);

        let length: u32 = u32::from_be_bytes([0x00, buf[3], buf[4], buf[5]]);

        let type_byte = reader.read_u8().await?;
        info!(target: "chunk::read", "message type: {}", type_byte);

        let mut buf: [u8; 4] = [0_u8; 4];
        reader.read_exact(&mut buf).await?;
        let message_stream_id = u32::from_le_bytes(buf);
        info!(target: "chunk::read", "message stream id: {}", message_stream_id);

        // buffer for message payload
        let mut message_buf: Vec<u8> = vec![0; length as usize];

        let bytes_read = reader
            .read_exact(&mut message_buf)
            .await
            .expect("read chunk message payload");
        if bytes_read < length as usize {
            warn!(target: "chunk::read", "bytes_read {} < length {}", bytes_read, length);
            panic!("expected {} bytes got {} bytes", length, bytes_read);
        }
        trace!(target: "chunk::read", "payload: {:02x?}", message_buf);
        let mut chunk_reader: &[u8] = &message_buf;

        let chunk: Chunk = match type_byte {
            1..=6 => Chunk::Control(Signal::read(&mut chunk_reader, type_byte).await?),
            20 => Chunk::Msg(Message {
                stream_id: message_stream_id,
                data: Message::read(&mut chunk_reader, type_byte, length).await?,
            }),
            8..=22 => panic!("unimplemented RTMP message type: {}", type_byte), // TODO: fail at some of these
            _ => panic!("unexpected chunk type: {}", type_byte),
        };
        Ok((chunk, header_size + length))
    }

    // 04 00 00 00 00 00 28 14  01 00 00 00               ......(.....
    //
    // 02 00 07 70 75 62 6c 69  73 68 00 40 10 00 00 00   ...publish.@....
    // 00 00 00 05 02 00 0a 63  61 6d 65 72 61 46 65 65   .......cameraFee
    // 64 02 00 04 4c 49 56 45                            d...LIVE

    pub async fn write<T>(mut writer: T, chunk: Chunk) -> io::Result<u32>
    where
        T: AsyncWrite + Unpin,
    {
        trace!(target: "chunk::write", "{:?}", chunk);
        let _bytes_written: u32 = 0;
        let mut buf = Vec::new();

        // get header info from message
        // set chunkstream ID based on message type
        match chunk {
            Chunk::Control(s) => panic!("unimplemented Chunk::Control {:?}", s),
            Chunk::Msg(message) => {
                let (mut cs_id, msg_type): (u8, u8) = match &message.data {
                    MessageData::Command(..) => (3, 0x14),
                    _ => {
                        warn!("unexpected message type {:?}, using csid=3", message);
                        (3, 0x14)
                    }
                };
                let stream_id = message.stream_id;
                if stream_id != 0 {
                    cs_id = 4;
                }
                // serialize the message into buffer to get its length
                Message::write(&mut buf, message)
                    .await
                    .expect("serialize message");
                // TODO: handle diff chunk headers/msg types
                // Type0 has 12 byte header (fmt/csid byte followed by 11 bytes)
                // example:
                //   04               Type 0, csid=4
                //   00 00 00         timestamp ()
                //   00 00 28         RTMP Message payload length: big endian / network
                //   14               RTMP Message type
                //   01 00 00 00      Message Stream ID: little endian
                let mut header = BufReadWriter::new(Vec::new());
                header.write_u8(cs_id).await.expect("write csid");
                let timestamp: u32 = 0; // TODO: get timestamp from message
                let timestamp_bytes = timestamp.to_be_bytes();
                header.write_u8(timestamp_bytes[1]).await.expect("ts2");
                header.write_u8(timestamp_bytes[2]).await.expect("ts1");
                header.write_u8(timestamp_bytes[3]).await.expect("ts0");

                let msg_len: u32 = buf.len().try_into().unwrap(); // TODO: check for 3
                trace!(target: "chunk::write", "msg_len: {:?}", msg_len);
                let len_bytes = msg_len.to_be_bytes();
                header.write_u8(len_bytes[1]).await.expect("len2");
                header.write_u8(len_bytes[2]).await.expect("len1");
                header.write_u8(len_bytes[3]).await.expect("len0");

                header.write_u8(msg_type).await.expect("write type");

                let stream_id_bytes = stream_id.to_le_bytes();
                header.write(&stream_id_bytes).await.expect("stream id");
                // hack
                // let hex_str = format!(
                //   "{:02x} 00 00 00 00 00 {:02x} 14  {:02x} 00 00 00",
                //   cs_id,
                //   buf.len(),
                //   stream_id
                // );
                // let chunk_header = util::bytes_from_hex_string(&hex_str);
                // writer
                //   .write(&chunk_header)
                //   .await
                //   .expect("write chunk header");
                trace!(target: "chunk::write", "header: {:02x?}", &header.buf);
                writer.write(&header).await.expect("write payload");

                trace!(target: "chunk::write", "payload: {:02x?}", &buf);
                writer.write(&buf).await.expect("write payload");
            } // Chunk::Msg
        }; // match chunk

        Ok(_bytes_written)
    } // fn write
} // impl Chunk

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;
    use crate::amf::Value;
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

        let flash_version = "MAC 10,0,32,18".to_string(); // TODO: must we, really?
        properties.insert("flashVer".to_string(), Value::Utf8(flash_version));

        properties.insert("objectEncoding".to_string(), Value::Number(0.0));

        let url = "rtmp://example.com/something".to_string();
        properties.insert("tcUrl".to_string(), Value::Utf8(url));

        let cmd = Message {
            stream_id: 0,
            data: MessageData::Command(MessageCommand {
                name: "connect".to_string(),
                id: 1.0,
                data: Value::Object(properties), // shouldn't this be an array?
                opt: Vec::new(),
            }),
        };

        let mut buf = Vec::new();

        let _num_bytes = Chunk::write(&mut buf, Chunk::Msg(cmd))
            .await
            .expect("write");
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
