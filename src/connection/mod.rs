use log::{info, trace, warn};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use url::Url;

use crate::amf::Value;
use crate::message::*;
use crate::stream::*;

mod inner;
use inner::InnerConnection;

// used by inner
// TODO: seems weird to "pub" when internal to module, but don't know syntax
pub mod bufreadwriter;
pub mod handshake;

// TODO: maybe this should be configurable?
const CHANNEL_SIZE: usize = 100;

type CommandsAwaitingResponse =
    HashMap<u64, oneshot::Sender<Result<MessageResponse, MessageError>>>;

#[derive(Clone, Debug)]
pub struct Connection {
    url: Url,
    next_cmd_id: Arc<AtomicUsize>,
    to_server_tx: Option<mpsc::Sender<Message>>, // messages destined server go here
    // stream_callback: fn(NetStream, Message) -> (),
    commands_awaiting_response: Arc<Mutex<CommandsAwaitingResponse>>,
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

impl Connection {
    // todo: new_with_transport, then new can defer creation of connection
    pub fn new(url: Url) -> Self {
        info!(target: "rtmp::Connection", "new");

        Connection {
            url,
            next_cmd_id: Arc::new(AtomicUsize::new(2)),
            to_server_tx: None,
            commands_awaiting_response: Default::default(), //Arc::new(Mutex::new(HashMap::new()))
        }
    }

    fn get_next_cmd_id(&self) -> f64 {
        // TODO: Handle rollover - look at wrapping_add
        self.next_cmd_id.fetch_add(1, Ordering::SeqCst) as f64
    }

    pub async fn send_command(
        &mut self,
        name: &str,
        params: Vec<Value>,
    ) -> Result<MessageResponse, MessageError> {
        let cmd_id = self.get_next_cmd_id();
        let mut to_server_tx = match &self.to_server_tx {
            Some(tx) => tx.clone(),
            None => panic!("need to be connected"),
        };

        let msg = Message::new(
            None,
            MessageData::Command(MessageCommand {
                name: name.to_string(),
                id: cmd_id,
                data: Value::Null,
                opt: params,
            }),
        );
        to_server_tx.send(msg).await?;

        let (sender, receiver) = oneshot::channel();
        let existing_subscriber = self
            .commands_awaiting_response
            .lock()
            .await
            .insert(cmd_id as u64, sender);
        if existing_subscriber.is_some() {
            warn!("ignoring this unexpected thing");
        }
        receiver.await?
    }

    pub async fn new_stream(&mut self) -> Result<(NetStream, MessageResponse), MessageError> {
        let msg = self.send_command("createStream", Vec::new()).await?;
        match msg {
            MessageResponse {
                opt: Value::Number(stream_id),
                ..
            } => Ok((NetStream::Created(self.clone(), stream_id as u32), msg)),
            MessageResponse { opt: _, .. } => Err(MessageError::new_status(
                "A.Bug",
                "The server isn't following the spec!",
            )),
        }

        // let msg = self.from_server_rx.filter(|msg| {
        //     message_indicating_success => true,
        //     message_indicating_failure => true,
        //     _ => false,
        // }).next().await;
        // if let msg = message_indicating_success {
        //     Ok(create_the_stream_from(msg))
        // } else {
        //     Err(msg)
        // }

        // let new_stream = NetStream::Command(cmd_id);
        // let mut stream_ref = self.stream.write().unwrap();

        // *stream_ref = new_stream.clone();
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
        let runtime = Handle::current();
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
        f: impl Fn(Connection, Message) -> () + Send + 'static,
        mut from_server_rx: mpsc::Receiver<Message>,
    ) {
        let runtime = Handle::current();
        let connection = self.clone();
        let _res_handle = runtime.spawn(async move {
            trace!(target: "rtmp:message_receiver", "spawn recv handler");
            let mut num: i32 = 1; // just for debugging
            loop {
                let msg = from_server_rx.recv().await.expect("recv from server");
                trace!(target: "rtmp:message_receiver", "#{}) recv from server {:?}", num, msg);

                if let Some(status) = msg.get_status() {
                    let v: Vec<&str> = status.code.split('.').collect();
                    match v[0] {
                        "NetConnection" => f(connection.clone(), msg),
                        _ => warn!(target: "rtmp:message_receiver", "unhandled status {:?}", status),
                    }
                } else {
                    match msg {
                        Message { data: MessageData::Response(response), .. } => {
                            let response_id = response.id;
                            if let Some(sender) = connection.commands_awaiting_response.lock().await.remove(&(response_id as u64)) {
                                if sender.send(Ok(response)).is_err() {
                                    warn!("Receiver for cmd {} went away", response_id);
                                }
                            } else {
                                warn!("Got a response to a command but we don't know what to do with it!");
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
        f: impl Fn(Connection, Message) -> () + Send + 'static,
    ) -> Result<(), Box<dyn std::error::Error>> {
        trace!(target: "rtmp:connect_with_callback", "url: {}", self.url);
        let (from_server_tx, from_server_rx) = mpsc::channel::<Message>(CHANNEL_SIZE);

        let (to_server_tx, to_server_rx) = mpsc::channel::<Message>(CHANNEL_SIZE);

        self.to_server_tx = Some(to_server_tx); // Connection methods use this to send messages to server

        self.spawn_socket_process_loop(to_server_rx, from_server_tx);
        self.spawn_message_receiver(f, from_server_rx);

        Ok(())
    }
}
