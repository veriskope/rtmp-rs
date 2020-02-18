mod flag;
pub use flag::RecordFlag;
use std::sync::RwLock;
use std::fmt;
use crate::chunk::Message;
use crate::amf::Value;

pub struct NetStreamInfo {
  name: String,
  flag: RecordFlag,
}


pub struct NetStream {
  id: u32,  // immutable, TODO: maybe stream should generate its own id?
  // publish happens rarely, typically once per stream
  // so RwLock allows for many readers
  info: RwLock<Option<NetStreamInfo>>,
  tx_to_server: std::sync::mpsc::Sender<Message>,
}

impl NetStream {
  pub fn new(id: u32,
            tx_to_server: std::sync::mpsc::Sender<Message>) -> Self {
    let msg = Message::Command { name: "createStream".to_string(),
              id: 0.0,
              data: Value::Null,
              opt: Vec::new() };    // this is silly
    // TODO: um... where do we put the stream_id?  does it come from server?
    tx_to_server.send(msg).expect("queue 'createStream' message to server");
    NetStream {
      id,
      info: RwLock::new(None),
      tx_to_server,
    }
  }

  pub fn publish(&self, name: String, flag: RecordFlag) {
    let  params = vec!(Value::Utf8(name.clone()), Value::Utf8(flag.to_string()));

    let mut info = self.info.write().unwrap();
    *info = Some( NetStreamInfo { name, flag } );

    let msg = Message::Command { name: "publish".to_string(),
              id: 0.0,
              data: Value::Null,
              opt: params };
    self.tx_to_server.send(msg).expect("queue 'publish' message to server");

  }

}

impl fmt::Display for NetStream {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let info_ref = self.info.read().unwrap();
    match &*info_ref {
      None => write!(f, "NetStream id{}: not published)", self.id),
      Some(info) =>  write!(f, "NetStream id{}: {} {})", self.id, info.name, info.flag)
    }
  }
}
