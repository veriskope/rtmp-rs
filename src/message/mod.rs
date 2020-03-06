use std::collections::HashMap;
use tokio::prelude::*;
extern crate proc_macro;
use crate::amf::Value;
use log::{info, trace, warn};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub stream_id: u32,
    pub data: MessageData,
}

pub const CONNECTION_CHANNEL: Option<u32> = None;

// ideally want to do type checking, can I make an enum that is really an Option?
// or does it need to be a new enum?
// enum Channel {
//     Connection = None,
//     Stream = Some(u32)
// }

pub const GENERATE: Option<u32> = None;

impl Message {
    pub fn new(stream_id: Option<u32>, data: MessageData) -> Self {
        Self {
            stream_id: stream_id.unwrap_or(0),
            data,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageData {
    // Video(MessageVideo),
    Command(MessageCommand),
    Response(MessageResponse),
    Status(MessageStatus),
    Error(MessageError),
}

// pub struct MessageVideo {
//     bytes: Box<[u8]>
//     timestamp: u32,
// }

#[derive(Debug, Clone, PartialEq)]
pub struct MessageCommand {
    pub name: String,
    pub id: f64,
    pub data: Value,
    pub opt: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageResponse {
    pub id: f64,
    pub data: Value,
    pub opt: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageStatus {
    pub level: String, // TODO: change to enum?
    pub code: String,
    pub description: String,
}

use tokio::sync::mpsc::error::SendError;
use tokio::sync::oneshot::error::RecvError;

#[derive(Clone, Debug, PartialEq)]
pub struct MessageError(pub MessageStatus);

impl MessageError {
    pub(crate) fn new_status(code: &str, description: &str) -> Self {
        Self(MessageStatus {
            level: "status".into(),
            code: code.into(),
            description: description.into(),
        })
    }
    pub(crate) fn new_error(code: &str, description: &str) -> Self {
        Self(MessageStatus {
            level: "error".into(),
            code: code.into(),
            description: description.into(),
        })
    }
}

impl<T> From<SendError<T>> for MessageError {
    fn from(_: SendError<T>) -> Self {
        Self::new_error(
            "Something.Closed",
            "The connection was previously closed due to an IO error or there's a bug",
        )
    }
}

impl From<RecvError> for MessageError {
    fn from(_: RecvError) -> Self {
        Self::new_error("A.Bug", "A command id was reused because of a bug")
    }
}

// enum StatusCode {
//     Success,
//     Failed,
//     Rejected,
//     Closed,
//     Unrecognized(String),
// }

// impl FromStr for StatusCode {
//     type Error = std::convert::Infallible;

//     fn from_str(s: &str) -> Result<Self, Self::Error> {
//         match s {

//         }
//     }
// }

// impl StatusCode {
//     fn as_str(&self) -> &str {
//         match *self {
//             StatusCode::Success => "...",

//         }
//     }
// }

/**
"NetConnection.Connect.Success"
"NetConnection.Connect.Failed"
"NetConnection.Connect.Rejected"
"NetConnection.Connect.Closed"
**/
impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.data {
            MessageData::Command(MessageCommand { name, id, .. }) => {
                write!(f, "Command '{}' #{}", name, id)
            }
            MessageData::Response(MessageResponse { id, .. }) => {
                write!(f, "Response '_result' #{}", id)
            }
            MessageData::Error(..) => write!(f, "Response '_error' #(oops unimplemented)"),
            MessageData::Status(MessageStatus {
                level,
                code,
                description,
            }) => write!(
                f,
                "Response 'onStatus' {}: {} \n   {}",
                level, code, description
            ),
        }
    }
}

//   AudioMessage         = 08,
//   VideoMessage         = 09,
//   DataMessage3         = 15,
//   SharedObjectMessage3 = 16,
//   CommandMessage3      = 17,       // 0x11
//   DataMessage0         = 18,
//   SharedObjectMessage0 = 19,
// CommandMessage0        = 20,       // 0x14
//   AggregateMessage     = 22,

// TODO: I've realized now that encoding is only returned with Connect response
//       which also has optional 'application' field
#[derive(Copy, Clone, Debug)]
pub struct Status<'a> {
    pub level: &'a str,
    pub code: &'a str,
    pub description: &'a str,
    pub encoding: u32,
}

impl Status<'_> {
    fn from_hash(h: &HashMap<String, Value>) -> Option<Status> {
        // expected...
        // {"code": Utf8("NetConnection.Connect.Success"),
        //  "level": Utf8("status"),
        //  "objectEncoding": Number(0.0),
        //   "description": Utf8("Connection succeeded.")}) }

        // TODO: return None if code or level is missing
        let level = Value::object_get_str(h, "level").unwrap_or("");

        let code = Value::object_get_str(h, "code").unwrap_or("");
        if code == "" {
            warn!(target: "Status::from_hash", "did not find code value in: {:?}", h);
        }

        let encoding = Value::object_get_number(h, "objectEncoding").unwrap_or(0.0) as u32;

        let description = Value::object_get_str(h, "description").unwrap_or("");

        Some(
            Status {
                level,
                code,
                description,
                encoding,
            }
            .to_owned(),
        )
    }
} // impl Status

impl Message {
    pub fn get_status(&self) -> Option<Status> {
        if let MessageData::Response(response_data) = &self.data {
            match &response_data.opt {
                Value::Object(h) => Status::from_hash(h),
                _ => None,
            }
        } else {
            None
        }
    }

    async fn read_command<T>(mut reader: T) -> io::Result<MessageData>
    where
        T: AsyncRead + Unpin,
    {
        let cmd_value = Value::read(&mut reader).await.expect("read command name");
        // trace!(target: "message::read", "cmd_value = {:?}", cmd_value);

        let transaction_id_value = Value::read(&mut reader).await.expect("read transaction id");
        trace!(target: "message::read", "transaction_id_value = {:?}", transaction_id_value);

        let data = Value::read(&mut reader).await.expect("read command data");
        trace!(target: "message::read", "command data = {:?}", data);

        if let Value::Number(id) = transaction_id_value {
            if let Value::Utf8(name) = cmd_value {
                let msg = match name.as_str() {
                    "_result" => {
                        let opt = Value::read(&mut reader).await.expect("read optional data");
                        trace!(target: "message::read", "_result optional data = {:?}", opt);
                        MessageData::Response(MessageResponse { id, data, opt })
                    }
                    "_error" => {
                        unimplemented!()
                        // let opt = Value::read(&mut reader).await.expect("read optional data");
                        // trace!(target: "message::read", "_result optional data = {:?}", opt);
                        // MessageError { id, data, opt }
                    }
                    "onStatus" => {
                        let opt = Value::read(&mut reader).await.expect("read optional data");
                        trace!(target: "message::read", "_result optional data = {:#?}", opt);
                        if let Value::Object(h) = opt {
                            let result = Status::from_hash(&h);
                            if let Some(status) = result {
                                MessageData::Status(MessageStatus {
                                    level: status.level.to_string(),
                                    code: status.code.to_string(),
                                    description: status.description.to_string(),
                                })
                            } else {
                                panic!("unexpected opt hash format {:?} in OnStatus id {}", h, id)
                            }
                        } else {
                            panic!("unexpected opt {:?} in OnStatus id {}", opt, id)
                        }
                    }

                    _ => {
                        // not sure what are different ways optional data can appear
                        // just reading one value for now, since that's what I've tested so far
                        let opt = match data {
                            Value::Null => Vec::new(),
                            _ => vec![Value::read(&mut reader).await.expect("read optional data")],
                        };
                        trace!(target: "message::read", "command optional data = {:?}", opt);
                        MessageData::Command(MessageCommand {
                            name,
                            id,
                            data,
                            opt,
                        })
                    }
                };
                return Ok(msg);
            } else {
                panic!("unexpected value for cmd {:?} ", cmd_value)
            }
        } else {
            panic!(
                "unexpected value for transaction_id {:?}",
                transaction_id_value
            )
        }
    }

    pub async fn read<T>(mut reader: T, chunk_type: u8, chunk_len: u32) -> io::Result<MessageData>
    where
        T: AsyncRead + Unpin,
    {
        info!(target: "message::read", "read chunk_type {:?}, chunk_len {:?}", chunk_type, chunk_len);

        // TODO: consider reading whole chunk?  or at least checking to see if we read correct amount?

        match chunk_type {
            20 => Self::read_command(&mut reader).await, // Command message AMF0
            _ => panic!("unimplemented read for message chunk type {}", chunk_type),
        } // match chunk_type
    } // pub async fn read

    pub async fn write<T>(mut writer: T, msg: Message) -> io::Result<()>
    where
        T: AsyncWrite + Unpin,
    {
        info!(target: "message::write", "Message: {:?}", msg);
        match msg.data {
            MessageData::Command(MessageCommand {
                name,
                id,
                data,
                opt,
            }) => {
                Value::write(&mut writer, Value::Utf8(name))
                    .await
                    .expect("write command name");
                Value::write(&mut writer, Value::Number(id.into()))
                    .await
                    .expect("write transaction id");
                Value::write(&mut writer, data)
                    .await
                    .expect("write command data");
                for val in opt {
                    Value::write(&mut writer, val)
                        .await
                        .expect("write optional info");
                }
            }
            _ => {
                panic!("unimplemented write for message {:?}", msg);
            }
        } // match chunk_type
        Ok(())
    } // pub async fn write
} // impl Message

#[cfg(test)]
mod tests {
    use super::*; // importing names from outer (for mod tests) scope.
    use crate::util::bytes_from_hex_string;
    use std::collections::HashMap;

    #[tokio::test]
    async fn can_read_command_response() {
        // this is really an integration test (ideally would be at a higher level)
        // std::env::set_var("RUST_LOG", "trace");
        // pretty_env_logger::init();
        let bytes = bytes_from_hex_string(
            "02 00 07 5f 72 65 73 75  6c 74
        00 3f f0 00 00 00 00 00 00
        03
        00 06 66 6d 73 56 65 72
        02 00 0f 46 4d 53 2f 35 2c 30 2c 31  35 2c 35 30 30 34
        00 0c 63 61 70 61 62 69 6c 69  74 69 65 73
        00 40 6f e0 00 00 00 00 00
        00 04 6d 6f 64 65
        00 3f f0 00 00 00 00 00 00
        00 00
        09
        02 00 01 58",
        );

        // 02 00 07 5f 72 65 73 75  6c 74  Utf8("_result")
        // 00 3f f0 00 00 00 00 00 00      Number(1.0)
        //
        //  Object({"mode": Number(1.0),
        //          "capabilities": Number(255.0),
        //          "fmsVer": Utf8String("FMS/5,0,15,5004")})
        //
        // 03                              Object marker
        //    00 06 66 6d  73 56 65 72                                 label: "fmsVer"
        //    02 00 0f 46 4d 53 2f 35 2c 30 2c 31  35 2c 35 30 30 34   value: Utf8(""FMS/5,0,15,5004"")
        //    00 0c 63 61 70 61 62 69 6c 69 74 69 65 73                label: "capabilities"
        //    00 40 6f e0 00 00 00 00 00                               value: Number(255.0)
        //    00 04 6d 6f 64 65                                        label: "mode"
        //    00 3f f0 00 00 00 00 00 00                               value: Number(1.0)
        //    00 00 09                     ObjectEnd
        //    02 00 01 58                  Utf8("X")
        //  Object({"level": Utf8String("status"),
        //           "code": Utf8String("NetConnection.Connect.Success"),
        //           "description": Utf8String("Connection succeeded."),

        //  64 61 74 61  "data"
        //           "data": Object({"version": Utf8String("5,0,15,5004")}),
        //           "objectEncoding": Number(0.0)})]
        // 03 00 05 6c 65 76 65 6c 02
        // 00 06 73 74 61 74 75 73  00 04 63 6f 64 65 02 00
        // 1d 4e 65 74 43 6f 6e 6e  65 63 74 69 6f 6e 2e 43
        // 6f 6e 6e 65 63 74 2e 53  75 63 63 65 73 73 00 0b
        // 64 65 73 63 72 69 70 74  69 6f 6e 02 00 15 43 6f
        // 6e 6e 65 63 74 69 6f 6e  20 73 75 63 63 65 65 64
        // 65 64 2e 00 0e 6f 62 6a  65 63 74 45 6e 63 6f 64
        // 69 6e 67 00 00 00 00 00  00 00 00 00 00 04
        // 64 61 74 61
        // 08
        // 00 00 00 00
        // 00 07 76 65 72 73 69 6f 6e
        // 02 00 0b 35 2c 30 2c 31  35 2c 35 30 30 34 00 00
        // 09
        // 00 00 09");

        let buf: &[u8] = &bytes;
        let m = Message::read(buf, 20, bytes.len() as u32)
            .await
            .expect("read");

        let mut data_hash = HashMap::new();
        data_hash.insert(
            "fmsVer".to_string(),
            Value::Utf8("FMS/5,0,15,5004".to_string()),
        );
        data_hash.insert("capabilities".to_string(), Value::Number(255.0));
        data_hash.insert("mode".to_string(), Value::Number(1.0));

        // let mut opt_hash = HashMap::new();

        // let mut nested_data = HashMap::new();
        // nested_data.insert("version".to_string(), Value::Utf8("5,0,15,5004".to_string()));
        // opt_hash.insert("data".to_string(), Value::Object(nested_data));

        // opt_hash.insert("description".to_string(), Value::Utf8("Connection succeeded.".to_string()));
        // opt_hash.insert("code".to_string(), Value::Utf8("NetConnection.Connect.Success".to_string()));
        // opt_hash.insert("level".to_string(), Value::Utf8("status".to_string()));

        assert_eq!(
            m,
            MessageData::Response(MessageResponse {
                id: 1.0,
                data: Value::Object(data_hash),
                opt: Value::Utf8("X".to_string())
            })
        );
    }

    #[tokio::test]
    async fn can_read_command_message_with_no_data() {
        pretty_env_logger::init();

        let bytes =
            bytes_from_hex_string("02 00 08 6f 6e 42 57 44 6f 6e 65 00 00 00 00 00 00 00 00 00 05");
        // 02                             Utf8 marker
        // 00 08                          length=8
        // 6f 6e 42 57 44 6f 6e 65        "onBWDone"
        // 00                             Number marker
        // 00 00 00 00 00 00 00 00        0
        // 05                             Null

        let buf: &[u8] = &bytes;
        let m = Message::read(buf, 20, bytes.len() as u32)
            .await
            .expect("read");

        let mut data_hash = HashMap::new();
        data_hash.insert(
            "fmsVer".to_string(),
            Value::Utf8("FMS/5,0,15,5004".to_string()),
        );
        data_hash.insert("capabilities".to_string(), Value::Number(255.0));
        data_hash.insert("mode".to_string(), Value::Number(1.0));

        assert_eq!(
            m,
            MessageData::Command(MessageCommand {
                name: "onBWDone".to_string(),
                id: 0.0,
                data: Value::Null,
                opt: Vec::new()
            }) // end Message::Command
        );
    }
} // mod tests
