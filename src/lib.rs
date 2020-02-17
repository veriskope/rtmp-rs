// used in amf.rs
#[macro_use] extern crate enum_primitive_derive;

use url::Url;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicU32, Ordering};

use tokio::prelude::*;
use tokio::{io::BufReader, net::TcpStream};
use tokio::runtime::Runtime;

pub mod error;
pub mod amf;
use amf::Value;

mod chunk;
use chunk::{Chunk, Signal, Message};

mod stream;
pub use stream::NetStream;
pub use stream::RecordFlag;

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

pub struct Connection {
  url: Url,
  runtime: Runtime,
  streams: HashMap<u32, Arc<NetStream>>,
  next_stream_id: Arc<AtomicU32>,
}

impl Connection {
  // todo: new_with_transport, then new can defer creation of connection
  pub fn new(url: Url) -> Self {
    info!(target: "rtmp::Connection", "new");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    Connection {
      url,
      runtime,
      streams: HashMap::new(),
      next_stream_id: Arc::new(AtomicU32::new(1)),
    }
  }

  pub fn new_stream(&mut self) -> Arc<NetStream> {
    // stream id can be anything
    // except 0 seems to be reserved for NetConnection
    // so we just start at 1 and increment the id when we need new ones
    let id = self.next_stream_id.fetch_add(1, Ordering::SeqCst);
    let ns = Arc::new(NetStream::new(id));
    self.streams.insert(id, ns.clone());

    //TODO self.inner.create_stream().await.expect("send command 'createStream'");

    ns
  }



  // std API that returns immediately, then calls callback later
  pub fn connect_with_callback(&mut self, f: impl Fn(Message) -> () + Send + 'static)
  -> Result<(), Box<dyn std::error::Error>>  {
    trace!(target: "rtmp:connect_with_callback", "url: {}", self.url);
    let (from_server_tx, from_server_rx) = std::sync::mpsc::channel::<Message>();
    // let (to_server_tx, to_server_rx) = std::sync::mpsc::channel::<Message>();
    // self.to_server_sender = Some(to_server_tx);

    let url = self.url.clone();
    let _cn_handle = self.runtime.spawn(async move {
      trace!(target: "rtmp:connect_with_callback", "spawn socket handler");
      let mut cn = InnerConnection::new(url).await;
      cn.connect().await.expect("rtmp connection");
      cn.read_loop(from_server_tx).await.expect("read until socket closes");
    });

    let _res_handle = self.runtime.spawn(async move {
      trace!(target: "rtmp:connect_with_callback", "spawn recv handler");
      let mut num = 1; // just for debugging
      loop {
        let msg = from_server_rx.recv().expect("recv from server");
        trace!(target: "rtmp:connect_with_callback", "#{}) recv from server {:?}", num, msg);
        if let Some(status) = msg.get_status() {
          let v: Vec<&str> = status.code.split('.').collect();
          match v[0] {
            "NetConnection" => f(msg),
            _ => warn!(target: "rtmp:connect_with_callback", "unhandled status {:?}", status),
          }
        } else {
          warn!(target: "rtmp:connect_with_callback", "unhandled message from server {:?}", msg);
        }
        num += 1;
      }
    });

    // this blocks forever (or until socket closed)
    // self.runtime.block_on(async move {
    //   _cn_handle.await.expect("waiting for cn");
    //   _res_handle.await.expect("waiting for res");
    // });

    Ok(())
  }

}

// private connection owned by read/write thread
struct InnerConnection {
    url: Url,
    cn: BufReader<TcpStream>,
    window_ack_size: u32,
    next_cmd_id: AtomicUsize,
}

impl InnerConnection {
  pub async fn new(url: Url) -> Self {
    let host = match url.host() {
      Some(h) => h,
      None => { panic!("host required") },
    };

    let port = url.port().unwrap_or(1935);
    let addr = format!("{}:{}", host, port);
    let tcp = TcpStream::connect(addr).await.expect("tcp connection failed");
    tcp.set_nodelay(true).expect("set_nodelay call failed");

    InnerConnection {
      url,
      cn: BufReader::new(tcp),
      window_ack_size: 2500000,
      next_cmd_id: AtomicUsize::new(1),
    }
  }

  fn get_next_cmd_id(&self) -> f64 {
    self.next_cmd_id.fetch_add(1, Ordering::SeqCst) as f64
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

  pub async fn create_stream(&mut self) -> io::Result<()> {
    let msg = Message::Command { name: "createStream".to_string(),
                                  id: self.get_next_cmd_id(),
                                  data: Value::Null,
                                  opt: Value::Null };

    Chunk::write(&mut self.cn, Chunk::Msg(msg)).await.expect("chunk write createStream command");
    Ok(())
  }

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
                                    id: self.get_next_cmd_id(),
                                    data: Value::Object(properties),
                                    opt: Value::Null };

      Chunk::write(&mut self.cn, Chunk::Msg(msg)).await.expect("chunk write");
      Ok(())
  }

  // handle protocol control messages, yield control and return
  // any message for
  pub async fn read_loop(&mut self, tx: std::sync::mpsc::Sender<Message>) -> io::Result<()> {
        // expected connect sequence
        // <---- Window Ack Size from server
        // <---- Set Peer Bandwidth from server
        // ----> Set Peer Bandwidth send to server
    loop {
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
            match m {
              // Command { name: String, id: u32, data: Value, opt: Value },

              Message::Response { id: _, data: _, opt: _} => {
                  trace!(target: "rtmp::Connection", "tx: {}", m);
                  tx.send(m).expect("transfer message to trigger callback");
              },
              _ => warn!(target: "rtmp::Connection", "unhandled message: {:?}", m),
            };
          }
          _ => warn!(target: "rtmp::Connection", "unhandled chunk: {:?}", chunk)
        }
      }
      // unreachable Ok(())
  }

  //pub async fn connect(&mut self) -> Result<(), Error> {
  // error[E0412]: cannot find type `Error` in this scope
  pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>>
  {
      let mut handshake = Handshake::new(PeerType::Client);
      let c0_and_c1 = handshake.generate_outbound_p0_and_p1().unwrap();
      self.cn.write(&c0_and_c1).await.expect("write c0_and_c1");

      loop {    // keep reading until we complete the handshake
        let mut read_buffer = [0_u8; 1024];
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
            let size = self.write(&response_bytes).await;
            if size == 0 { panic!("unexpected socket close"); }
          }
        if is_finished {
          trace!(target: "rtmp::connect", "handshake completed");
          break;
        }
      }
      self.send_connect_command().await.expect("send_connect_command");

      Ok(())
  }

}



// Invoking createStream
// 14                                    Command
// 02                                    Utf8 marker
// 00 0c                                 length = 12
// 63 72 65 61 74  65 53 74 72 65 61 6d  createStream
// 00                                    Number marker
// 40 00 00 00 00 00 00 00               1.0?
// 05                                    Null


// publish
// 14  00 00 00 00               ......$.....
// 02 00 07 70 75 62 6c 69  73 68 00 40 08 00 00 00   ...publish.@....
// 00 00 00 05 02 00 06 73  61 6d 70 6c 65 02 00 04   .......sample...
// 4c 49 56 45                                        LIVE


