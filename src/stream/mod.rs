mod flag;
use crate::amf::Value;
use crate::message::*;
use crate::Connection;
pub use flag::RecordFlag;
use log::{trace, warn};
use std::fmt;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, RwLock};

#[derive(Clone)]
pub struct NetStream {
    id: u32,
    cn: Connection,
    pub notify: mpsc::Sender<MessageStatus>,
    state: Arc<RwLock<NetStreamState>>,
}

impl fmt::Debug for NetStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        futures::executor::block_on(async move {
            let state_ref = self.state.read().await;
            write!(
                f,
                "NetStream {{ id: {}, state: {:?} }}",
                self.id, *state_ref
            )
        })
    }
}

#[derive(Clone, Debug)]
pub enum NetStreamState {
    Created,
    PublishRequest(PublishInfo),
    Published(PublishInfo),
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
    pub fn new(id: u32, cn: Connection) -> Self {
        let (tx, mut rx) = mpsc::channel::<MessageStatus>(100);
        let new_stream = Self {
            id,
            cn,
            notify: tx,
            state: Default::default(),
        };

        let stream = new_stream.clone(); // xfer to closure
        Handle::current().spawn(async move {
            loop {
                let option = rx.recv().await; // why would this ever be None?
                trace!(target: "NetStream::receiver", "recv stream option = {:#?}", option);
                if let Some(msg_status) = option {
                    // MessageStatus for stream
                    trace!(target: "NetStream::receiver", "recv stream msg = {:#?}", msg_status);

                    if msg_status.code == "NetStream.Publish.Start" {
                        {
                            let mut state_ref = stream.state.write().await;
                            trace!(target: "NetStream::receiver", "recv stream *state_ref = {:?}", *state_ref);
                            let old_state = (*state_ref).clone();
                            if let PublishRequest(info) = old_state {
                                *state_ref = Published(info.clone());
                            } else {
                                warn!("state should be PublishRequest");
                            }
                        }
                        trace!(target: "NetStream::receiver", "end Some(msg_status), stream: {:?}", stream);
                    }
                }
            }
        });
        new_stream
    }

    pub async fn publish(&mut self, name: &str, flag: RecordFlag) -> Result<(), MessageError> {
        trace!(target: "NetStream::publish", "{}: {}", name, flag);
        let mut state_ref = self.state.write().await;

        let result = match *state_ref {
            Created => {
                let params = vec![Value::Utf8(name.into()), Value::Utf8(flag.to_string())];
                *state_ref = PublishRequest(PublishInfo {
                    name: name.to_string(),
                    flag,
                });
                drop(state_ref);
                let response = self
                    .cn
                    .send_stream_command(Some(self.id), "publish", params)
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
