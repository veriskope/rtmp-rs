mod flag;
use crate::amf::Value;
use crate::Message;
pub use flag::RecordFlag;
use log::trace;

pub enum NetStream {
    // idea for a state machine, not sure this is the right approach
    Command(f64),
    Created(f64),
    // PublishRequest(String, RecordFlag),
}

// pub struct NetStreamInfo {
//     name: String,
//     flag: RecordFlag,
// }

pub fn create_stream(cmd_id: f64, tx_to_server: std::sync::mpsc::Sender<Message>) {
    let msg = Message::Command {
        name: "createStream".to_string(),
        id: cmd_id,
        data: Value::Null,
        opt: Vec::new(),
    };
    tx_to_server
        .send(msg)
        .expect("queue 'createStream' message to server");
}

pub fn publish(
    stream_id: u32,
    tx_to_server: std::sync::mpsc::Sender<Message>,
    name: String,
    flag: RecordFlag,
) {
    let params = vec![Value::Utf8(name.clone()), Value::Utf8(flag.to_string())];

    let msg = Message::StreamCommand {
        name: "publish".to_string(),
        stream_id,
        params,
    };
    trace!(target: "publish", "tx_to_server: {:?}", msg);
    tx_to_server
        .send(msg)
        .expect("queue 'publish' message to server");
}

// impl fmt::Display for NetStream {
//   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//     let info_ref = self.info.read().unwrap();
//     match &*info_ref {
//       None => write!(f, "NetStream id{}: not published)", self.id),
//       Some(info) => write!(f, "NetStream id{}: {} {})", self.id, info.name, info.flag),
//     }
//   }
