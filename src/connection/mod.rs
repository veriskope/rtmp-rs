use log::{info, trace, warn};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
    HashMap<u32, oneshot::Sender<Result<MessageResponse, MessageError>>>;

type StreamsAwaitingStatus = HashMap<u32, mpsc::Sender<MessageStatus>>;

#[derive(Clone, Debug)]
pub struct Connection {
    url: Url,
    is_connected: Arc<AtomicBool>,
    next_cmd_id: Arc<AtomicU32>,
    to_server_tx: Option<mpsc::Sender<Message>>, // messages destined server go here
    // stream_callback: fn(NetStream, Message) -> (),
    commands_awaiting_response: Arc<Mutex<CommandsAwaitingResponse>>,
    streams_awaiting_status: Arc<Mutex<StreamsAwaitingStatus>>,
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
            is_connected: Default::default(),
            next_cmd_id: Arc::new(AtomicU32::new(2)),
            to_server_tx: None,
            commands_awaiting_response: Default::default(),
            streams_awaiting_status: Default::default(),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::SeqCst)
    }

    pub fn set_connected(&self, connected: bool) {
        self.is_connected.fetch_or(connected, Ordering::SeqCst);
    }

    // get_next_cmd_id generates a unique Command transaction id
    // for all outstanding requests
    // can be recycled once a response has been received
    // allows for 2^32 requests, typically there will be only a few
    fn get_next_cmd_id(&self) -> u32 {
        // fetch_add wraps around on overflow
        self.next_cmd_id.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn send_command(
        &mut self,
        name: &str,
        params: Vec<Value>,
    ) -> Result<MessageResponse, MessageError> {
        self.send_raw_command(CONNECTION_CHANNEL, name, GENERATE, Value::Null, params)
            .await
    }

    pub async fn send_raw_command(
        &mut self,
        stream_id: Option<u32>,
        name: &str,
        transaction_id: Option<u32>, // if not given, generate one
        data: Value,
        opt: Vec<Value>,
    ) -> Result<MessageResponse, MessageError> {
        let cmd_id = transaction_id.unwrap_or_else(|| self.get_next_cmd_id());
        let mut to_server_tx = match &self.to_server_tx {
            Some(tx) => tx.clone(),
            None => panic!("need to be connected"),
        };

        let id: f64 = cmd_id.into();
        let msg = Message::new(
            stream_id,
            MessageData::Command(MessageCommand {
                name: name.to_string(),
                id,
                data,
                opt,
            }),
        );
        to_server_tx.send(msg).await?;

        let (sender, receiver) = oneshot::channel();
        let existing_subscriber = self
            .commands_awaiting_response
            .lock()
            .await
            .insert(cmd_id, sender);
        if existing_subscriber.is_some() {
            warn!("ignoring unexpected response for {:?}", existing_subscriber);
        }
        receiver.await?
    }

    // connect must have transaction ID #1
    async fn send_connect_command(
        &mut self,
        // params: Vec<Value>, TODO: check API, caller can send some kind of server params, not sure where they go
    ) -> Result<MessageResponse, MessageError> {
        let mut properties = HashMap::new();
        let mut url_path = self.url.path_segments().unwrap();
        let app_name = url_path.next().unwrap();
        properties.insert("app".to_string(), Value::Utf8(app_name.to_string()));
        let flash_version = "MAC 10,0,32,18".to_string(); // TODO: must we, really?
        properties.insert("flashVer".to_string(), Value::Utf8(flash_version));
        // properties.insert("objectEncoding".to_string(), Amf0Value::Number(0.0));
        properties.insert("tcUrl".to_string(), Value::Utf8(self.url.to_string()));

        self.send_raw_command(
            CONNECTION_CHANNEL,
            "connect",
            Some(1),
            Value::Object(properties),
            Vec::new(),
        )
        .await
    }

    // streams use transaction id = 0, success or error happens via status
    pub async fn send_stream_command(
        &mut self,
        stream_id: u32,
        name: &str,
        params: Vec<Value>,
    ) -> Result<(), MessageError> {
        let mut to_server_tx = match &self.to_server_tx {
            Some(tx) => tx.clone(),
            None => panic!("need to be connected"),
        };

        let msg = Message::new(
            Some(stream_id),
            MessageData::Command(MessageCommand {
                name: name.to_string(),
                id: 0.0,
                data: Value::Null,
                opt: params,
            }),
        );
        to_server_tx.send(msg).await?;
        Ok(())
    }

    pub async fn new_stream(&mut self) -> Result<(NetStream, MessageResponse), MessageError> {
        let msg = self.send_command("createStream", Vec::new()).await?;
        match msg {
            MessageResponse {
                opt: Value::Number(stream_id),
                ..
            } => {
                let id = stream_id as u32;
                let stream = NetStream::new(id, self.clone());
                self.streams_awaiting_status
                    .lock()
                    .await
                    .insert(id, stream.notify.clone());
                Ok((stream, msg))
            }
            MessageResponse { opt: _, .. } => Err(MessageError::new_status(
                "NetStream.Create.Failed", // TODO: check consistency
                "Server did not provide a stream id number",
            )),
        }
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
            trace!(target: "rtmp:spawn_socket_process_loop", "creating socket connection");
            // maybe InnerConnection is chunkstream?
            let mut cn = InnerConnection::new(url, to_server_rx).await;

            trace!(target: "rtmp:spawn_socket_process_loop", "chunkstream connected");
            cn.process_message_loop(from_server_tx)
                .await
                .expect("read until socket closes");
        });
    }

    pub fn spawn_message_receiver(&mut self, mut from_server_rx: mpsc::Receiver<Message>) {
        let runtime = Handle::current();
        let connection = self.clone();
        let _res_handle = runtime.spawn(async move {
            trace!(target: "rtmp:message_receiver", "spawn recv handler");
            let mut num: i32 = 1; // just for debugging
            loop {
                let msg = from_server_rx.recv().await.expect("recv from server");
                trace!(target: "rtmp:message_receiver", "#{}) recv from server {:?}", num, msg);
                match msg {
                    Message { data: MessageData::Response(response), .. } => {
                    let cmd_id = response.id as u32;

                        if let Some(sender) = connection.commands_awaiting_response.lock().await.remove(&cmd_id) {
                            if sender.send(Ok(response)).is_err() {
                                warn!("Receiver for cmd {} went away", cmd_id);
                            }
                        } else {
                            warn!("Got a response for unregistered command {}", cmd_id);
                        }
                    },
                    Message { stream_id, data: MessageData::Status(status) } => {
                        trace!(target: "rtmp:message_receiver", "status: {:?}", status);
                        if stream_id == 0 {
                            panic!("unexpected status with stream id 0");   // TODO: I think this means it is a NetConnection status message
                        } else {
                            if let Some(sender) = connection.streams_awaiting_status.lock().await.get_mut(&stream_id) {
                                trace!(target: "rtmp:message_receiver", "sending...");

                                if sender.send(status).await.is_err() {
                                    warn!("Stream Receiver for stream id #{} went away", stream_id);
                                } else {
                                    trace!(target: "rtmp:message_receiver", "sent!");
                                }
                            } else {
                                warn!("Got a status on stream {} with no receiver!", stream_id);
                            }
                        }

                    },
                    _ => {
                        warn!(target: "rtmp:connect_with_callback", "unhandled message from server {:?}", msg)
                    }
                }
                num += 1;
            }
        });
    }

    pub async fn connect(&mut self) -> Result<MessageResponse, MessageError> {
        let (from_server_tx, from_server_rx) = mpsc::channel::<Message>(CHANNEL_SIZE);

        let (to_server_tx, to_server_rx) = mpsc::channel::<Message>(CHANNEL_SIZE);

        self.to_server_tx = Some(to_server_tx); // Connection methods use this to send messages to server

        self.spawn_socket_process_loop(to_server_rx, from_server_tx);
        self.spawn_message_receiver(from_server_rx);
        // queue the connect command, then await ...
        // - handshake completion
        // - queued command gets sent in process_message_loop
        // - receive connect response
        let response = self.send_connect_command().await;
        if let Ok(msg) = &response {
            if let Some(ref status) = msg.get_status() {
                trace!(target: "rtmp::Connection", "connect command response: {:?}", status);
                if status.code == "NetConnection.Connect.Success" {
                    trace!(target: "rtmp::Connection", "setting is_connected: {}", true);
                    self.set_connected(true);
                } else {
                    self.set_connected(false);
                }
            }
        }
        response
    }

    // std API that returns immediately, then calls callback later
    pub fn connect_with_callback(
        &mut self,
        f: impl Fn(Connection, Message) -> () + Send + 'static,
    ) -> Result<(), Box<dyn std::error::Error>> {
        trace!(target: "rtmp:connect_with_callback", "url: {}", self.url);

        // just block for now, while refactoring
        futures::executor::block_on(async move {
            let response = self.connect().await;
            // TODO: need to figure out client error handling expectations
            if let Ok(msg) = response {
                trace!(target: "rtmp:connect_with_callback", "connected: {}, {:#?}", self.is_connected(), msg);
                // want: Message::from_data(msg)
                f(
                    self.clone(),
                    Message {
                        stream_id: 0,
                        data: MessageData::Response(msg.clone()),
                    },
                );
            }
        });

        Ok(())
    }
}
