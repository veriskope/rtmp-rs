use log::{trace, warn};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use url::Url;

use tokio::prelude::*;
use tokio::sync::mpsc;
use tokio::{io::BufReader, net::TcpStream};

use crate::amf::Value;
use crate::chunk::{Chunk, Signal};
use crate::Message;

use super::bufreadwriter::BufReadWriter;
use super::handshake::{Handshake, HandshakeProcessResult, PeerType};

// private connection owned by read/write thread
pub struct InnerConnection {
  url: Url,
  rx_to_server: mpsc::Receiver<Message>,
  cn: BufReadWriter<BufReader<TcpStream>>,
  window_ack_size: u32,
  chunk_size: u32,
  is_connected: AtomicBool,
}

impl InnerConnection {
  pub async fn new(url: Url, rx_to_server: mpsc::Receiver<Message>) -> Self {
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
      chunk_size: 1024, // TODO: is this a good default?
      is_connected: AtomicBool::new(false),
    }
  }

  fn is_connected(&self) -> bool {
    self.is_connected.load(Ordering::SeqCst)
  }

  // async fn send_connect_command(&mut self) -> Result<(), Box<dyn std::error::Error>> {
  async fn send_connect_command(&mut self) -> io::Result<()> {
    trace!(target: "rtmp::Connection", "send_connect_command");

    let mut properties = HashMap::new();
    let mut url_path = self.url.path_segments().unwrap();
    let app_name = url_path.next().unwrap();
    properties.insert("app".to_string(), Value::Utf8(app_name.to_string()));
    let flash_version = "MAC 10,0,32,18".to_string(); // TODO: must we, really?
    properties.insert("flashVer".to_string(), Value::Utf8(flash_version));
    // properties.insert("objectEncoding".to_string(), Amf0Value::Number(0.0));
    properties.insert("tcUrl".to_string(), Value::Utf8(self.url.to_string()));

    let msg = Message::Command {
      name: "connect".to_string(),
      id: 1.0,
      data: Value::Object(properties),
      opt: Vec::new(),
    };

    Chunk::write(&mut self.cn.buf, Chunk::Msg(msg))
      .await
      .expect("chunk write");
    Ok(())
  }

  async fn handle_chunk(&mut self, chunk: Chunk, mut tx: mpsc::Sender<Message>) -> io::Result<()> {
    trace!(target: "rtmp::Connection", "{:?}", chunk);
    match chunk {
      Chunk::Control(Signal::SetWindowAckSize(size)) => {
        self.window_ack_size = size;
        warn!(target: "rtmp::Connection",
        "SetWindowAckSize - set window_ack_size {:?}", size)
      }
      Chunk::Control(Signal::SetPeerBandwidth(size, _limit)) => {
        self.window_ack_size = size;
        warn!(target: "rtmp::Connection", "SetPeerBandwidth - set window_ack_size {:?}", size)
      }
      Chunk::Control(Signal::SetChunkSize(size)) => {
        self.chunk_size = size;
        warn!(target: "rtmp::Connection", "SetChunkSize - set chunk_size {:?}", size)
      }
      Chunk::Control(Signal::UserControlMessage(event_type)) => {
        warn!(target: "rtmp::Connection", "UserControlMessage {:?} - unhandled", event_type)
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
                trace!(target: "rtmp::Connection", "setting is_connected: {}", true);
                self.is_connected.fetch_or(true, Ordering::SeqCst);
              } // TODO: check for disconnect
            }
            tx.send(m)
              .await
              .expect("transfer message to trigger callback");
          }
          _ => warn!(target: "rtmp::Connection", "unhandled message: {:?}", m),
        };
      }
      _ => warn!(target: "rtmp::Connection", "unhandled chunk: {:?}", chunk),
    }
    Ok(())
  }
  // read messages from TCPStream
  // - handle protocol control messages
  // - send RTMP messages via tx
  // after connecting to server, then handle sending messages
  // - recv messages (via self.rx_to_server) and write as chunks
  pub async fn process_message_loop(&mut self, tx: mpsc::Sender<Message>) -> io::Result<()> {
    // This note totally belongs somewhere else now, just not sure where!
    // expected connect sequence
    // <---- Window Ack Size from server
    // <---- Set Peer Bandwidth from server
    // ----> Set Peer Bandwidth send to server
    loop {
      if self.is_connected() {
        tokio::select! {
            Some(outgoing_msg) = self.rx_to_server.recv()  => {
              trace!(target: "rtmp::Connection", "outgoing message: {:?}", outgoing_msg);
              // TODO: this shouldn't be synchronous
              Chunk::write(&mut self.cn.buf, Chunk::Msg(outgoing_msg))
                  .await
                  .expect("chunk write message from queue");
            }
            Ok(response) = Chunk::read(&mut self.cn.buf) => {
                let (chunk, _num_bytes) = response;
                self.handle_chunk(chunk, tx.clone()).await.expect("handle chunk");
            }
        }
      } else {
        let (chunk, _num_bytes) = Chunk::read(&mut self.cn.buf).await?;
        self
          .handle_chunk(chunk, tx.clone())
          .await
          .expect("handle chunk");
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
