mod flag;
use crate::amf::Value;
use crate::message::*;
use crate::Connection;
pub use flag::RecordFlag;
// use futures::sink::Sink;
use futures::stream::{Stream, StreamExt};
use log::trace;
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::runtime::Handle;
use tokio::sync::mpsc;

pub struct NetStream {
    pub id: u32,
    cn: Connection,
    messages: mpsc::Receiver<MessageStatus>,
    state: NetStreamState,
}

impl Stream for NetStream {
    type Item = MessageStatus;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.messages.poll_next_unpin(cx)
    }
}

impl Drop for NetStream {
    fn drop(&mut self) {
        let cn = self.cn.clone();
        let id = self.id;
        Handle::current().spawn(async move {
            cn.remove_stream(id).await;
        });
    }
}

// impl Sink<MessageData> for NetStream {
//     type Error = std::convert::Infallible;

//     fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
//         self.connection.poll_stream_ready(cx)
//     }

//     fn start_send(self: Pin<&mut Self>, item: MessageData) -> Result<(), Self::Error> {
//         unimplemented!()
//     }

//     fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
//         Poll::Ready(Ok(()))
//     }

//     fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
//         Poll::Ready(Ok(()))
//     }
// }

impl fmt::Debug for NetStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NetStream {{ id: {}, state: {:?} }}",
            self.id, self.state
        )
    }
}

#[derive(Clone, Debug)]
pub enum NetStreamState {
    Created,
    PublishRequest(PublishInfo),
    // Published(PublishInfo),
}

impl Default for NetStreamState {
    fn default() -> Self {
        NetStreamState::Created
    }
}

#[derive(Clone, Debug)]
pub struct PublishInfo {
    name: String,
    flag: RecordFlag,
}

use NetStreamState::*;
impl NetStream {
    // // called for every status message received on this stream
    // async fn handle_message(&self, msg_status: MessageStatus) {
    //     // MessageStatus for stream

    //     if msg_status.code == "NetStream.Publish.Start" {
    //         {
    //             let mut state_ref = self.state.write().await;
    //             trace!(target: "NetStream::receiver", "recv stream *state_ref = {:?}", *state_ref);
    //             let old_state = (*state_ref).clone();
    //             if let PublishRequest(info) = old_state {
    //                 *state_ref = Published(info.clone());
    //             } else {
    //                 warn!("state should be PublishRequest");
    //             }
    //         }
    //         trace!(target: "NetStream::receiver", "end Some(msg_status), stream: {:?}", self);
    //     }
    // }

    pub async fn new(id: u32, cn: Connection) -> Self {
        let messages = cn.add_stream(id).await;
        Self {
            id,
            cn,
            messages,
            state: Default::default(),
        }
    }

    // TODO: Rename `publish_request` or change waits to receive
    // message from server and return success/failure message
    pub async fn publish(&mut self, name: &str, flag: RecordFlag) -> Result<(), MessageError> {
        trace!(target: "NetStream::publish", "{}: {}", name, flag);
        let result = match self.state {
            Created => {
                let params = vec![Value::Utf8(name.into()), Value::Utf8(flag.to_string())];
                self.state = PublishRequest(PublishInfo {
                    name: name.to_string(),
                    flag,
                });
                let response = self
                    .cn
                    .send_stream_command(self.id, "publish", params)
                    .await?;
                trace!("{:?}", response);
                Ok(response)
            }
            _ => unimplemented!(),
        };
        trace!(target: "NetStream::publish", "publish request sent: {:?}", self);
        result
    }
}

// pub struct NetStreamInfo {
//     name: String,
//     flag: RecordFlag,
// }

// pub async fn publish(
//   stream_id: u32,
//   mut tx_to_server: mpsc::Sender<Message>,
//   name: String,
//   flag: RecordFlag,
// ) {
//   let params = vec![Value::Utf8(name.clone()), Value::Utf8(flag.to_string())];

//   let msg = Message::StreamCommand {
//     name: "publish".to_string(),
//     stream_id,
//     params,
//   };
//   trace!(target: "publish", "tx_to_server: {:?}", msg);
//   tx_to_server
//     .send(msg)
//     .await
//     .expect("queue 'publish' message to server");
// }

// impl fmt::Display for NetStream {
//   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//     let info_ref = self.info.read().unwrap();
//     match &*info_ref {
//       None => write!(f, "NetStream id{}: not published)", self.id),
//       Some(info) => write!(f, "NetStream id{}: {} {})", self.id, info.name, info.flag),
//     }
//   }
