// used om amf0.rs
#[macro_use] extern crate enum_primitive_derive;


use tokio::prelude::*;
use tokio::{io::BufReader, net::TcpStream};
pub mod error;
mod chunk;
use chunk::{Chunk, Signal};

mod handshake;
use handshake::{HandshakeProcessResult, Handshake, PeerType};

use log::{info, warn};
use rml_amf0::{Amf0Value};
use std::collections::HashMap;

pub mod util {
use log::{info};

//fn bytes_from_hex_string(hex_str: &str) -> &[u8] {
pub fn bytes_from_hex_string(hex_str: &str) -> Vec<u8> {
  // info!(target: "util::bytes_from_hex_string", "hex_str: {}", hex_str);
  println!("hex_str: {}", hex_str);
  let result = hex_str.split_whitespace()
                  .map(|hex_digit| u8::from_str_radix(hex_digit, 16))
                  .collect::<Result<Vec<u8>, _>>();
  match result {
    Ok(bytes) => {
      info!(target: "util", "{:02X?}", bytes);
      println!("bytes: {:?}", bytes);
      return bytes
    },
    Err(e) => panic!("error {} parsing: {}", e, hex_str),
  }
  // b"0x01"
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
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
} // mod util


pub struct Connection<T = TcpStream> {
    url: String,
    cn: BufReader<T>,
    window_ack_size: u32,
}

impl<Transport: AsyncRead + AsyncWrite + Unpin> Connection<Transport> {
    // consider passing ConnectionOptions with transport? and (URL | parts)
    pub fn new(url: String, transport: Transport) -> Self {
      info!(target: "rtmp::Connection", "new");
      Connection {
        url,
        cn: BufReader::new(transport),
        window_ack_size: 2500000,
      }
    }

    async fn write(&mut self, bytes: &[u8]) -> usize {
        let bytes_written = self.cn.write(bytes).await.expect("write");
        if bytes_written == 0 {
            panic!("connection unexpectedly closed");   // TODO: return real error
        } else {
            info!(target: "rtmp::Connection", "wrote {} bytes", bytes_written);
        }
        self.cn.flush().await.expect("send_command: flush after write");
        return bytes_written
    }

    // pub async fn send_command(&mut self, command_name: String, params: &[Amf0Value]) -> Result<(), Box<dyn std::error::Error>> {

    // async fn send_connect_command(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    async fn send_connect_command(&mut self) -> io::Result<()> {
        info!(target: "rtmp::Connection", "send_connect_command");
        let mut properties = HashMap::new();

        let command_name = "connect".to_string();
        let app = "vod/media".to_string();
        properties.insert("app".to_string(), Amf0Value::Utf8String(app));
        let flash_version = "MAC 10,0,32,18".to_string();   // TODO: must we, really?
        properties.insert("flashVer".to_string(), Amf0Value::Utf8String(flash_version));
        // properties.insert("objectEncoding".to_string(), Amf0Value::Number(0.0));
        properties.insert("tcUrl".to_string(), Amf0Value::Utf8String(self.url.clone()));

        //let params = [Amf0Value::Object(properties)];
        // TODO: Amf0Value - should implement Copy, & API shouldn't need copy

        let arguments = Amf0Value::Object(properties);

        let transaction_id:f64 = 1.0;   // connect message is always 1, TODO: increment later
        let values = vec![
            Amf0Value::Utf8String(command_name),
            Amf0Value::Number(transaction_id),
            arguments       // connect has one param, should allow array
        ];

        let bytes = rml_amf0::serialize(&values).expect("serialize command argument");
        println!("{:02x?}", bytes);
        let message_length = bytes.len();
        println!("length: {}", message_length);


        // hardcode chunkstream message header
        let cs_id: u8 = 3;
        let hex_str = format!("{:02x} 00 00 00 00 00 {:02x} 14  00 00 00 00", cs_id, message_length);
        let chunk_header = util::bytes_from_hex_string(&hex_str);
        self.write(&chunk_header).await;

        self.write(&bytes).await;
        loop {
          // expected connect sequence
          // <---- Window Ack Size from server
          // <---- Set Peer Bandwidth from server
          // ----> Set Peer Bandwidth send to server
          // write chunk

          let (chunk, num_bytes) = Chunk::read(&mut self.cn).await?;
          info!(target: "rtmp::Connection", "{:?}", chunk);
          info!(target: "rtmp::Connection", "parsed {} bytes", num_bytes);
          match chunk {
            Chunk::Control(Signal::SetWindowAckSize(size)) => {
              self.window_ack_size = size;
              warn!(target: "rtmp::Connection", "SetWindowAckSize - saving variable, but functionality is unimplemented")
            },
            Chunk::Control(Signal::SetPeerBandwidth(size, _limit)) => {
              self.window_ack_size = size;
              warn!(target: "rtmp::Connection", "ignoring bandwidth limit request")
            },
            _ => warn!(target: "rtmp::Connection", "unhandled chunk: {:?}", chunk)
          }
        }
        // unreachable Ok(())
    }

    //pub async fn connect(&mut self) -> Result<(), Error> {
    // error[E0412]: cannot find type `Error` in this scope
    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut handshake = Handshake::new(PeerType::Client);
        let c0_and_c1 = handshake.generate_outbound_p0_and_p1().unwrap();
        self.cn.write(&c0_and_c1).await.expect("write c0_and_c1");

        let mut read_buffer = [0_u8; 1024];
        loop {
            let num_bytes = self.cn.read(&mut read_buffer).await.expect("read");
            if num_bytes == 0 {
                panic!("connection unexpectedly closed");   // TODO: return real error?
            }
            println!("bytes read: {}", num_bytes);
                let (is_finished, response_bytes) = match handshake.process_bytes(&read_buffer) {
                Err(x) => panic!("Error returned: {:?}", x),
                Ok(HandshakeProcessResult::InProgress {response_bytes: bytes}) => (false, bytes),
                Ok(HandshakeProcessResult::Completed {response_bytes: bytes, remaining_bytes: _}) => (true, bytes)
            };
            if response_bytes.len() > 0 {
                self.write(&response_bytes).await; 
            }
            if is_finished {
              self.send_connect_command().await?;
              break;
            }
        }
        Ok(())
    }



}

