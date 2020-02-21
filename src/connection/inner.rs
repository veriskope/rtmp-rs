use log::{trace, warn};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use url::Url;

use tokio::prelude::*;
use tokio::{io::BufReader, net::TcpStream};

use crate::amf::Value;
use crate::chunk::{Chunk, Signal};
use crate::Message;

use super::bufreadwriter::BufReadWriter;
use super::handshake::{Handshake, HandshakeProcessResult, PeerType};

// private connection owned by read/write thread
pub struct InnerConnection {
  url: Url,
  rx_to_server: std::sync::mpsc::Receiver<Message>,
  cn: BufReadWriter<BufReader<TcpStream>>,
  window_ack_size: u32,
  next_cmd_id: AtomicUsize,
  is_connected: AtomicBool,
}

impl InnerConnection {
  pub async fn new(url: Url, rx_to_server: std::sync::mpsc::Receiver<Message>) -> Self {
    let host = match url.host() {
      Some(h) => h,
      None => panic!("host required"),
    };

    let port = url.port().unwrap_or(1935);
    let addr = format!("{}:{}", host, port);
    let tcp = TcpStream::connect(addr)
      .await
      .expect("tcp connection failed");
    tcp.set_nodelay(true).expect("set_nodelay call failed");

    InnerConnection {
      url,
      rx_to_server,
      cn: BufReadWriter::new(BufReader::new(tcp)),
      window_ack_size: 2500000,
      next_cmd_id: AtomicUsize::new(1),
      is_connected: AtomicBool::new(false),
    }
  }

  fn is_connected(&self) -> bool {
    self.is_connected.load(Ordering::SeqCst)
  }

  fn get_next_cmd_id(&self) -> f64 {
    self.next_cmd_id.fetch_add(1, Ordering::SeqCst) as f64
  }

  // async fn send_connect_command(&mut self) -> Result<(), Box<dyn std::error::Error>> {
  async fn send_connect_command(&mut self) -> io::Result<()> {
    trace!(target: "rtmp::Connection", "send_connect_command");

    let mut properties = HashMap::new();
    let app_name = "vod/media".to_string();
    properties.insert("app".to_string(), Value::Utf8(app_name));
    let flash_version = "MAC 10,0,32,18".to_string(); // TODO: must we, really?
    properties.insert("flashVer".to_string(), Value::Utf8(flash_version));
    // properties.insert("objectEncoding".to_string(), Amf0Value::Number(0.0));
    properties.insert("tcUrl".to_string(), Value::Utf8(self.url.to_string()));

    let msg = Message::Command {
      name: "connect".to_string(),
      id: self.get_next_cmd_id(),
      data: Value::Object(properties),
      opt: Vec::new(),
    };

    Chunk::write(&mut self.cn.buf, Chunk::Msg(msg))
      .await
      .expect("chunk write");
    Ok(())
  }
  // read messages from TCPStream
  // - handle protocol control messages
  // - send RTMP messages via tx
  // after connecting to server, then handle sending messages
  // - recv messages (via self.rx_to_server) and write as chunks
  pub async fn process_message_loop(
    &mut self,
    tx: std::sync::mpsc::Sender<Message>,
  ) -> io::Result<()> {
    // expected connect sequence
    // <---- Window Ack Size from server
    // <---- Set Peer Bandwidth from server
    // ----> Set Peer Bandwidth send to server
    loop {
      if self.is_connected() {
        // if we've connected to the server, then check if we have a request to send a message to the server
        let result = self.rx_to_server.try_recv();
        if let Ok(mut outgoing_msg) = result {
          // TODO: this shouldn't be synchronous
          if let Message::Command {
            name,
            id: _,
            data,
            opt,
          } = outgoing_msg
          {
            // make sure we have unique transaction ids
            outgoing_msg = Message::Command {
              name,
              id: self.get_next_cmd_id(),
              data,
              opt,
            };
          };
          Chunk::write(&mut self.cn.buf, Chunk::Msg(outgoing_msg))
            .await
            .expect("chunk write message from queue");
        }
      }
      let (chunk, num_bytes) = Chunk::read(&mut self.cn.buf).await?;
      trace!(target: "rtmp::Connection", "{:?}", chunk);
      trace!(target: "rtmp::Connection", "parsed {} bytes", num_bytes);
      match chunk {
        Chunk::Control(Signal::SetWindowAckSize(size)) => {
          self.window_ack_size = size;
          warn!(target: "rtmp::Connection", "SetWindowAckSize - saving variable, but functionality is unimplemented")
        }
        Chunk::Control(Signal::SetPeerBandwidth(size, _limit)) => {
          self.window_ack_size = size;
          warn!(target: "rtmp::Connection", "ignoring bandwidth limit request")
        }
        Chunk::Msg(m) => {
          match m {
            // Command { name: String, id: u32, data: Value, opt: Value },
            Message::Response {
              id: _,
              data: _,
              opt: _,
            } => {
              trace!(target: "rtmp::Connection", "tx: {}", m);
              if let Some(status) = m.get_status() {
                if status.code == "NetConnection.Connect.Success" {
                  self.is_connected.fetch_or(true, Ordering::SeqCst);
                } // TODO: check for disconnect
              }
              tx.send(m).expect("transfer message to trigger callback");
            }
            _ => warn!(target: "rtmp::Connection", "unhandled message: {:?}", m),
          };
        }
        _ => warn!(target: "rtmp::Connection", "unhandled chunk: {:?}", chunk),
      }
    }
    // unreachable Ok(())
  }

  async fn connect_handshake(&mut self) -> io::Result<()> {
    let mut handshake = Handshake::new(PeerType::Client);
    let c0_and_c1 = handshake.generate_outbound_p0_and_p1().unwrap();
    self.cn.write(&c0_and_c1).await.expect("write c0_and_c1");

    loop {
      // keep reading until we complete the handshake
      let mut read_buffer = [0_u8; 1024];
      let num_bytes = self.cn.read(&mut read_buffer).await.expect("read");
      if num_bytes == 0 {
        panic!("connection unexpectedly closed"); // TODO: return real error?
      }
      trace!(target: "rtmp::connect", "bytes read: {}", num_bytes);
      let (is_finished, response_bytes) = match handshake.process_bytes(&read_buffer) {
        Err(x) => panic!("Error returned: {:?}", x),
        Ok(HandshakeProcessResult::InProgress {
          response_bytes: bytes,
        }) => (false, bytes),
        Ok(HandshakeProcessResult::Completed {
          response_bytes: bytes,
          remaining_bytes: _,
        }) => (true, bytes),
      };
      if response_bytes.len() > 0 {
        self
          .cn
          .write_exact(&response_bytes)
          .await
          .expect("handshake");
      }
      if is_finished {
        trace!(target: "rtmp::connect", "handshake completed");
        return Ok(());
      }
    }
  }

  //pub async fn connect(&mut self) -> Result<(), Error> {
  // error[E0412]: cannot find type `Error` in this scope
  pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    self.connect_handshake().await.expect("handshake");
    self.send_connect_command().await.expect("connect command");
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
