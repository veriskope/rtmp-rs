use log::{info, trace, warn};
use std::sync::{Arc, Mutex};

use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use url::Url;

use crate::amf::Value;
use crate::stream::*;
use crate::Message;

mod inner;
use inner::InnerConnection;

// used by inner
// TODO: seems weird to "pub" when internal to module, but don't know syntax
pub mod bufreadwriter;
pub mod handshake;

// TODO: maybe this should be configurable?
const CHANNEL_SIZE: usize = 100;

pub struct Connection {
    url: Url,
    runtime_guard: Arc<Mutex<Runtime>>,
    next_cmd_id: AtomicUsize,
    to_server_tx: mpsc::Sender<Message>, // messages destined for server go here
    to_server_rx: Option<mpsc::Receiver<Message>>, // process loop grabs them from here
    stream_callback: fn(Message) -> (),
}

// how to support closure as well as functions?
//     stream_callback: dyn Fn(Message) -> (),
//
// 35 |     pub fn new(url: Url, stream_callback: dyn Fn(Message) -> ()) -> Self {
//    |                          ^^^^^^^^^^^^^^^ doesn't have a size known at compile-time
//    |
//    = help: the trait `std::marker::Sized` is not implemented for `(dyn std::ops::Fn(message::Message) + 'static)`
//    = note: to learn more, visit <https://doc.rust-lang.org/book/ch19-04-advanced-types.html#dynamically-sized-types-and-the-sized-trait>
//    = note: all local variables must have a statically known size
//    = help: unsized locals are gated as an unstable feature

// error[E0277]: the size for values of type `(dyn std::ops::Fn(message::Message) + 'static)` cannot be known at compilation time
//   --> src/connection/mod.rs:35:69
//    |
// 35 |     pub fn new(url: Url, stream_callback: dyn Fn(Message) -> ()) -> Self {
//    |                                                                     ^^^^ doesn't have a size known at compile-time
//    |

fn default_stream_callback(msg: Message) {
    println!("default stream callback for {:?}", msg);
}
impl Connection {
    // todo: new_with_transport, then new can defer creation of connection
    pub fn new(url: Url, maybe_stream_callback: Option<fn(Message) -> ()>) -> Self {
        info!(target: "rtmp::Connection", "new");
        let stream_callback = match maybe_stream_callback {
            Some(cb) => cb,
            None => default_stream_callback,
        };
        let runtime = tokio::runtime::Runtime::new().expect("new Runtime");
        let (to_server_tx, to_server_rx) = mpsc::channel::<Message>(CHANNEL_SIZE);

        Connection {
            url,
            runtime_guard: Arc::new(Mutex::new(runtime)),
            next_cmd_id: AtomicUsize::new(2),
            to_server_tx,
            to_server_rx: Some(to_server_rx), // consumed by process_loop
            stream_callback,
        }
    }

    fn get_next_cmd_id(&self) -> f64 {
        self.next_cmd_id.fetch_add(1, Ordering::SeqCst) as f64
    }

    pub fn new_stream(&mut self) -> NetStream {
        // looks like stream id is created on the server
        let cmd_id = self.get_next_cmd_id();
        let to_server_tx = self.to_server_tx.clone();
        let mut runtime = self.runtime_guard.lock().unwrap();

        runtime.block_on(async move {
            create_stream(cmd_id, to_server_tx).await;
        });
        NetStream::Command(cmd_id)
    }

    //     to_server_rx: ownership moves to the spawned thread, its job is to
    //                  recv messages on this channel and send 'em to the server
    //  from_server_tx: the thread also listens on the socket, reads messages
    //                  and sends them on this channel
    fn spawn_socket_process_loop(
        &mut self,
        to_server_rx: mpsc::Receiver<Message>,
        from_server_tx: mpsc::Sender<Message>,
    ) {
        let url = self.url.clone();
        let mut runtime = self.runtime_guard.lock().unwrap();
        let _cn_handle = runtime.spawn(async move {
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
        mut from_server_rx: mpsc::Receiver<Message>,
    ) {
        let to_server_tx = self.to_server_tx.clone();
        let mut runtime = self.runtime_guard.lock().unwrap();

        let _res_handle = runtime.spawn(async move {
      trace!(target: "rtmp:message_receiver", "spawn recv handler");
      let mut num: i32 = 1; // just for debugging
      loop {
        let msg = from_server_rx.recv().await.expect("recv from server");
        trace!(target: "rtmp:message_receiver", "#{}) recv from server {:?}", num, msg);
        if let Some(status) = msg.get_status() {
          let v: Vec<&str> = status.code.split('.').collect();
          match v[0] {
            "NetConnection" => f(msg),
            _ => warn!(target: "rtmp:message_receiver", "unhandled status {:?}", status),
          }
        } else {
          match msg {
            Message::Response { id, data: _, opt } => {
              if id == 2.0 {
                match opt {
                  Value::Number(stream_id) => {
                    // create stream
                    trace!(target: "rtmp:message_receiver", "publish");
                    publish(
                      stream_id as u32,
                      to_server_tx.clone(),
                      "cameraFeed".to_string(),
                      RecordFlag::Live,
                    )
                    .await;
                  }
                  _ => {
                    warn!(target: "rtmp:message_receiver", "unexpected opt type {:?}", opt);
                  }
                } // match opt
              } // id == 2.0
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
        let (from_server_tx, from_server_rx) = mpsc::channel::<Message>(CHANNEL_SIZE);
        // ownership will move to thread for processing messages
        let to_server_rx_option = std::mem::replace(&mut self.to_server_rx, None);
        let to_server_rx = to_server_rx_option.expect("to_server_rx should be defined");

        self.spawn_socket_process_loop(to_server_rx, from_server_tx);
        self.spawn_message_receiver(f, from_server_rx);

        Ok(())
    }
}
