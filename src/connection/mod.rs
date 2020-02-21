use log::{info, trace, warn};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use url::Url;

use crate::stream::*;
use crate::Message;
use tokio::runtime::Runtime;

mod inner;
use inner::InnerConnection;

// used by inner
// TODO: seems weird to "pub" when internal to module, but don't know syntax
pub mod bufreadwriter;
pub mod handshake;

pub struct Connection {
  url: Url,
  runtime: Runtime,
  next_cmd_id: AtomicUsize,
  to_server_tx: std::sync::mpsc::Sender<Message>, // messages destined for server go here
  to_server_rx: Option<std::sync::mpsc::Receiver<Message>>, // process loop grabs them from here
}

impl Connection {
  // todo: new_with_transport, then new can defer creation of connection
  pub fn new(url: Url) -> Self {
    info!(target: "rtmp::Connection", "new");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (to_server_tx, to_server_rx) = std::sync::mpsc::channel::<Message>();

    Connection {
      url,
      runtime,
      next_cmd_id: AtomicUsize::new(2),
      to_server_tx,
      to_server_rx: Some(to_server_rx), // consumed by process_loop
    }
  }

  fn get_next_cmd_id(&self) -> f64 {
    self.next_cmd_id.fetch_add(1, Ordering::SeqCst) as f64
  }

  pub fn new_stream(&mut self) -> NetStream {
    // looks like stream id is created on the server
    let cmd_id = self.get_next_cmd_id();
    create_stream(cmd_id, self.to_server_tx.clone());
    NetStream::Command(cmd_id)
  }

  //     to_server_rx: ownership moves to the spawned thread, its job is to
  //                  recv messages on this channel and send 'em to the server
  //  from_server_tx: the thread also listens on the socket, reads messages
  //                  and sends them on this channel
  fn spawn_socket_process_loop(
    &mut self,
    to_server_rx: std::sync::mpsc::Receiver<Message>,
    from_server_tx: std::sync::mpsc::Sender<Message>,
  ) {
    let url = self.url.clone();
    let _cn_handle = self.runtime.spawn(async move {
      trace!(target: "rtmp:connect_with_callback", "spawn socket handler");
      let mut cn = InnerConnection::new(url, to_server_rx).await;
      cn.connect().await.expect("rtmp connection");
      cn.process_message_loop(from_server_tx)
        .await
        .expect("read until socket closes");
    });
  }

  pub fn spawn_message_receiver(
    &mut self,
    f: impl Fn(Message) -> () + Send + 'static,
    from_server_rx: std::sync::mpsc::Receiver<Message>,
  ) {
    let to_server_tx = self.to_server_tx.clone();
    let _res_handle = self.runtime.spawn(async move {
      trace!(target: "rtmp:message_receiver", "spawn recv handler");
      let mut num = 1; // just for debugging
      loop {
        let msg = from_server_rx.recv().expect("recv from server");
        trace!(target: "rtmp:message_receiver", "#{}) recv from server {:?}", num, msg);
        if let Some(status) = msg.get_status() {
          let v: Vec<&str> = status.code.split('.').collect();
          match v[0] {
            "NetConnection" => f(msg),
            _ => warn!(target: "rtmp:message_receiver", "unhandled status {:?}", status),
          }
        } else {
          match msg {
            Message::Response { id, data, opt } => {
              if id == 2.0 {
                // create stream
                trace!(target: "rtmp:message_receiver", "publish");
                publish(
                  3.0,
                  to_server_tx.clone(),
                  "cameraFeed".to_string(),
                  RecordFlag::Live,
                );
              }
            }
            _ => {
              warn!(target: "rtmp:connect_with_callback", "unhandled message from server {:?}", msg)
            }
          }
        }
        num += 1;
      }
    });
  }

  // std API that returns immediately, then calls callback later
  pub fn connect_with_callback(
    &mut self,
    f: impl Fn(Message) -> () + Send + 'static,
  ) -> Result<(), Box<dyn std::error::Error>> {
    trace!(target: "rtmp:connect_with_callback", "url: {}", self.url);
    let (from_server_tx, from_server_rx) = std::sync::mpsc::channel::<Message>();
    // ownership will move to thread for processing messages
    let to_server_rx_option = std::mem::replace(&mut self.to_server_rx, None);
    let to_server_rx = to_server_rx_option.expect("to_server_rx should be defined");

    self.spawn_socket_process_loop(to_server_rx, from_server_tx);
    self.spawn_message_receiver(f, from_server_rx);

    Ok(())
  }
}
