mod flag;
use crate::amf::Value;
use crate::message::*;
use crate::Connection;
pub use flag::RecordFlag;
use log::trace;

#[derive(Clone, Debug)]
pub struct NetStream {
    id: u32,
    cn: Connection,
    status: NetStreamState,
}

#[derive(Clone, Debug)]
pub enum NetStreamState {
    Created,
    Published(String, RecordFlag),
}

impl NetStream {
    pub fn new(id: u32, cn: Connection) -> Self {
        Self {
            id,
            cn,
            status: NetStreamState::Created,
        }
    }

    pub async fn publish(
        &mut self,
        name: &str,
        flag: RecordFlag,
    ) -> Result<MessageResponse, MessageError> {
        use NetStreamState::*;

        match self.status {
            Created => {
                let params = vec![Value::Utf8(name.into()), Value::Utf8(flag.to_string())];
                let response = self.cn.send_command("publish", params).await?;
                trace!("{:?}", response);
                self.status = Published(name.into(), flag);
                Ok(response)
            }
            _ => unimplemented!(),
        }
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
