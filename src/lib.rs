// used om amf0.rs
#[macro_use] extern crate enum_primitive_derive;

use url::Url;
use tokio::prelude::*;
use tokio::{io::BufReader, net::TcpStream};
pub mod error;
mod chunk;
use chunk::{Chunk, Signal, Message, Value};

mod handshake;
use handshake::{HandshakeProcessResult, Handshake, PeerType};

use log::{info, trace, warn};
use std::collections::HashMap;

pub mod util {
use log::{trace};

//fn bytes_from_hex_string(hex_str: &str) -> &[u8] {
pub fn bytes_from_hex_string(hex_str: &str) -> Vec<u8> {
  trace!(target: "util::bytes_from_hex_string", "hex_str: {}", hex_str);
  let result = hex_str.split_whitespace()
                  .map(|hex_digit| u8::from_str_radix(hex_digit, 16))
                  .collect::<Result<Vec<u8>, _>>();
  match result {
    Ok(bytes) => {
      trace!(target: "util", "{:02X?}", bytes);
      return bytes
    },
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
} // mod util


pub struct Connection<T = TcpStream> {
    url: Url,
    cn: BufReader<T>,
    window_ack_size: u32,
}

impl<Transport: AsyncRead + AsyncWrite + Unpin> Connection<Transport> {
    // consider passing ConnectionOptions with transport? and (URL | parts)
    pub fn new(url: Url, transport: Transport) -> Self {
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
        trace!(target: "rtmp::Connection", "send_connect_command");

        let mut properties = HashMap::new();
        let app_name = "vod/media".to_string();
        properties.insert("app".to_string(), Value::Utf8(app_name));
        let flash_version = "MAC 10,0,32,18".to_string();   // TODO: must we, really?
        properties.insert("flashVer".to_string(), Value::Utf8(flash_version));
        // properties.insert("objectEncoding".to_string(), Amf0Value::Number(0.0));
        properties.insert("tcUrl".to_string(), Value::Utf8(self.url.to_string()));

        let msg = Message::Command { name: "connect".to_string(),
                                     id: 1,
                                     data: Value::Object(properties),
                                     opt: Value::Null };

        Chunk::write(&mut self.cn, Chunk::Msg(msg)).await.expect("chunk write");

        loop {
          // expected connect sequence
          // <---- Window Ack Size from server
          // <---- Set Peer Bandwidth from server
          // ----> Set Peer Bandwidth send to server
          // write chunk

          let (chunk, num_bytes) = Chunk::read(&mut self.cn).await?;
          trace!(target: "rtmp::Connection", "{:?}", chunk);
          trace!(target: "rtmp::Connection", "parsed {} bytes", num_bytes);
          match chunk {
            Chunk::Control(Signal::SetWindowAckSize(size)) => {
              self.window_ack_size = size;
              warn!(target: "rtmp::Connection", "SetWindowAckSize - saving variable, but functionality is unimplemented")
            },
            Chunk::Control(Signal::SetPeerBandwidth(size, _limit)) => {
              self.window_ack_size = size;
              warn!(target: "rtmp::Connection", "ignoring bandwidth limit request")
            },
            Chunk::Msg(m) => {
              warn!(target: "rtmp::Connection", "got message: {:?}", m);
              // need to propagate to client
            }
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
            trace!(target: "rtmp::connect", "bytes read: {}", num_bytes);
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

