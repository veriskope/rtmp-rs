mod flag;
// use crate::amf::Value;
use crate::message::*;
use crate::Connection;
pub use flag::RecordFlag;
// use log::trace;
// use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub enum NetStream {
    Created(Connection, u32),
    // PublishRequest(String, RecordFlag),
}

impl NetStream {
    pub async fn publish(&mut self, name: &str, flag: RecordFlag) -> Result<(), MessageError> {
        unimplemented!()
        // let msg = self.send_command("createStream", Vec::new()).await?;
        // match msg {
        //     Message::Response {
        //         opt: Value::Number(stream_id),
        //         ..
        //     } => Ok((NetStream::Created(self.clone(), stream_id as u32), msg)),
        //     Message::Response { opt: _, .. } => Err(ErrorMessage::new_status(
        //         "A.Bug",
        //         "The server isn't following the spec!",
        //     )),
        //     _ => panic!("unimplemented!"),
        // }
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
