use log::{info, trace, warn};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use url::Url;

use crate::stream::NetStream;
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
  // TODO: keep track of streams so we can clean them up?
  next_stream_id: Arc<AtomicU32>,
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
      next_stream_id: Arc::new(AtomicU32::new(1)),
      to_server_tx,
      to_server_rx: Some(to_server_rx), // consumed by process_loop
    }
  }

  pub fn new_stream(&mut self) -> NetStream {
    // stream id can be anything
    // except 0 seems to be reserved for NetConnection
    // so we just start at 1 and increment the id when we need new ones
    let id = self.next_stream_id.fetch_add(1, Ordering::SeqCst);
    NetStream::new(id, self.to_server_tx.clone())
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

    let url = self.url.clone();
    let _cn_handle = self.runtime.spawn(async move {
      trace!(target: "rtmp:connect_with_callback", "spawn socket handler");
      let mut cn = InnerConnection::new(url, to_server_rx).await;
      cn.connect().await.expect("rtmp connection");
      cn.process_message_loop(from_server_tx)
        .await
        .expect("read until socket closes");
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
