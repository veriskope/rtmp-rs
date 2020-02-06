use tokio::prelude::*;
extern crate proc_macro;
use log::{info, trace};
use super::amf::Value;

// TODO: can we just derive Read on these, given that we know type?
#[derive(Debug, PartialEq)]
pub enum Message {
  //Placeholder,
  // TODO: is highest f64 integer < u32?
  Command { name: String, id: u32, data: Value, opt: Value },
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

impl Message {
  pub async fn read<T>(mut reader: T, chunk_type: u8, chunk_len: u32) -> io::Result<Message>
  where T: AsyncRead + Unpin
  {
    info!(target: "message::read", "read chunk_type {:?}, chunk_len {:?}", chunk_type, chunk_len);

    // TODO: consider reading whole chunk?  or at least checking to see if we read correct amount?

    match chunk_type {
        20 => {     // Command message AMF0
          let cmd_value = Value::read(&mut reader).await.expect("read command name");
          trace!(target: "message::read", "cmd_value = {:?}", cmd_value);

          let transaction_id_value = Value::read(&mut reader).await.expect("read transaction id");
          trace!(target: "message::read", "transaction_id_value = {:?}", transaction_id_value);
          let data = Value::read(&mut reader).await.expect("read command data");
          trace!(target: "message::read", "command data = {:?}", data);
          let opt = Value::read(&mut reader).await.expect("read optional data");
          trace!(target: "message::read", "optional data = {:?}", opt);

          // = note: for more information, see https://github.com/rust-lang/rust/issues/53667
          // = help: add `#![feature(let_chains)]` to the crate attributes to enable
          //if let Value::Utf8(cmd_value) = name && if let Value::Number(transaction_id_value) = transaction_id {

          if let Value::Utf8(name) = cmd_value  {
            if let Value::Number(float_id) = transaction_id_value {
              let id: u32 = float_id as u32;
              let cmd = Message::Command{ name, id, data, opt};
              info!(target: "message::read", "cmd: {:?}", cmd);
              return Ok(cmd)
            } else {
              panic!("unexpected value for transaction_id {:?}", transaction_id_value)
            }
          } else {
            panic!("unexpected value for cmd {:?} ", cmd_value)
          }
        },
        _ => {
          panic!("unimplemented read for message chunk type {}", chunk_type)
        }
    } // match chunk_type
  } // pub async fn read
} // impl Message

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;
    use crate::util::bytes_from_hex_string;
    use std::collections::HashMap;

  #[tokio::test]
  async fn can_read_command_message() {
    // this is really an integration test (ideally would be at a higher level)
    // std::env::set_var("RUST_LOG", "trace");
    // std::env::set_var("RUST_LOG", "info");
    pretty_env_logger::init();
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
      02 00 01 58");


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
      let m = Message::read(buf, 20, bytes.len() as u32).await.expect("read");

      let mut data_hash = HashMap::new();
      data_hash.insert("fmsVer".to_string(), Value::Utf8("FMS/5,0,15,5004".to_string()));
      data_hash.insert("capabilities".to_string(), Value::Number(255.0));
      data_hash.insert("mode".to_string(), Value::Number(1.0));

      // let mut opt_hash = HashMap::new();

      // let mut nested_data = HashMap::new();
      // nested_data.insert("version".to_string(), Value::Utf8("5,0,15,5004".to_string()));
      // opt_hash.insert("data".to_string(), Value::Object(nested_data));

      // opt_hash.insert("description".to_string(), Value::Utf8("Connection succeeded.".to_string()));
      // opt_hash.insert("code".to_string(), Value::Utf8("NetConnection.Connect.Success".to_string()));
      // opt_hash.insert("level".to_string(), Value::Utf8("status".to_string()));

        assert_eq!(m, Message::Command{
                      name: "_result".to_string(),
                      id: 1,
                      data: Value::Object(data_hash),
                      opt: Value::Utf8("X".to_string())
                  }
                );
    }


} // mod tests