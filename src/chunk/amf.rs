use log::{info, trace};
use std::collections::HashMap;
use tokio::prelude::*;

extern crate num_traits;
use num_traits::FromPrimitive;

#[derive(Debug, Primitive, PartialEq)]
enum Marker {
  Number          = 0,      // f64: 8 byte IEEE-754 double precision floating point
  Boolean         = 1,      // bool
  Utf8String      = 2,      // String
  Object          = 3,      // HashMap
  // Movieclip       = 4,
  Null            = 5,
  // Undefined       = 6,
  // Reference       = 7,
  EcmaArray       = 8,
  ObjectEnd       = 9,
  // StrictArray     = 10,
  // Date            = 11,
  // LongUtf8String  = 12,
  // Unsupported     = 13,
  // Recordset       = 14,
  // XmlDocument     = 15,
  // TypedObject     = 16,
}
//TODO:   AvmPlusObject   = 17,    // 0x11

// const EMPTY_STRING: u16 = 0x00;

use Marker::*;

#[derive(Debug, PartialEq)]
pub enum Value {
  Number(f64),
  Boolean(bool),
  Utf8(String),
  Object(HashMap<String, Value>),
  Null,
  // Undefined,
}
impl Value {
  async fn read_string<T>(mut reader: T) -> io::Result<String>
  where T: AsyncRead + Unpin
  {
    trace!(target: "amf::Value::read_string", "--- fn read_string");
    let len = reader.read_u16().await.expect("read string length");
    if len == 0 {
      trace!(target: "amf::Value::read_string", "empty string");
      return Ok("".to_string())
    }

    trace!(target: "amf::Value::read_string", "length: {:02x?}", len);
    let mut s:Vec<u8> = Vec::with_capacity(len.into());
    for _i in 0..len {
        s.push(0x00);
    }

    let num_bytes = reader.read_exact(&mut s)
                      .await.expect("read string bytes");
    trace!(target: "amf::Value::read_string", "string bytes: {:02x?}", s);

    if num_bytes != (len as usize) {
      panic!("expected string length to be {}, got {}", len, num_bytes);
    }
    let utf8_string = String::from_utf8(s).expect("utf8 parse");
    trace!(target: "amf::Value::read_string", "string text: {:?}", utf8_string);
    Ok(utf8_string)
  }

  // .take copies reader rather than mutating it -- probably a bug
  async fn read_number<T>(mut reader: T) -> io::Result<f64>
  where T: AsyncRead + Unpin
  {
    trace!(target: "amf::Value::read_number", "--- fn read_number");
    let mut bytes: [u8; 8] = [0x00; 8];
    let num_bytes = reader.read_exact(&mut bytes)
                      .await.expect("read number bytes");
    trace!(target: "amf::Value::read_number", "bytes: {:02x?}", bytes);

    if num_bytes != 8 {
      panic!("expected Number length to be 8, got {}", num_bytes);
    }
    trace!(target: "amf::Value::read_number", "f64: {:?}", f64::from_be_bytes(bytes));
    Ok(f64::from_be_bytes(bytes))
  }

  async fn read_bool<T>(mut reader: T) -> io::Result<bool>
  where T: AsyncRead + Unpin
  {
    trace!(target: "amf::Value::read_bool", "--- fn read_bool");
    let byte = reader.read_u8()
                      .await.expect("read boolean byte");
    // true if not false (zero value is false)
    trace!(target: "amf::Value::read_bool", "byte: {:?}, bool: {:?}", byte, byte != 0x00);
    Ok(byte != 0x00)
  }

  async fn read_leaf_value<T>(mut reader: T) -> io::Result<(Marker, Option<Value>)>
  where T: AsyncRead + Unpin
  {
    trace!(target: "amf::Value::read_leaf_value", "fn read_leaf_value");
    let marker_byte = reader.read_u8().await.expect("read AMF type marker");
    trace!(target: "amf::Value::read_leaf_value", "marker byte: {:02x?}", marker_byte);
    if let Some(marker) = Marker::from_u8(marker_byte) {
      let leaf_value = match marker {
        Utf8String => {
          let s = Value::read_string(&mut reader).await.expect("read Amf0 Utf8String");
          Some(Value::Utf8(s))
        },
        Number => {
          let n = Value::read_number(&mut reader).await.expect("read Amf0 Number");
          Some(Value::Number(n))
        },
        Boolean => {
          let b = Value::read_bool(&mut reader).await.expect("read Amf0 Boolean");
          Some(Value::Boolean(b))
        },
        _ => {
          if marker != ObjectEnd && marker != Object && marker != EcmaArray {
            panic!("unimplemented type {:?}", marker);
          }
          None
        }
      }; // match Marker
      trace!(target: "amf::Value::read_leaf_value", "marker: {:?} leaf_value: {:?}", marker, leaf_value);
      Ok((marker, leaf_value))
    } else {
      panic!("unexpected marker byte: {:?}", marker_byte);
    }
  }


  async fn read_object<T>(mut reader: T) -> io::Result<HashMap<String, Value>>
  where T: AsyncRead + Unpin
  {
    trace!(target: "amf::Value::read_object", "--- fn read_object");
    let mut obj_hash: HashMap<String, Value> = HashMap::new();
    let mut done = false;
    let mut inner_done = false;
    while !done {
      let name = Value::read_string(&mut reader).await.expect("read object property name");
      let (marker, leaf_value) = Value::read_leaf_value(&mut reader).await.expect("read leaf value");
      if let Some(val) = leaf_value {
        obj_hash.insert(name, val);
      } else if marker == Object || marker == EcmaArray {    // this is dumb, should unroll into a loop :)
        if marker == EcmaArray {
          let fake_len = reader.read_u32().await.expect("ecma array length -- unused?");
          trace!(target: "amf::Value::read_object", "ignoring ecma length u32 {:?}", fake_len);
        }
        let mut inner_hash: HashMap<String, Value> = HashMap::new();
        while !inner_done {
          let name = Value::read_string(&mut reader).await.expect("read object property name");
          let (marker, leaf_value) = Value::read_leaf_value(&mut reader).await.expect("read leaf value");

          if let Some(val) = leaf_value {
            inner_hash.insert(name, val);
          }
          if marker == Object || marker == EcmaArray {
            panic!("nested too deep")
          }
          if marker == ObjectEnd {
            inner_done = true;
            trace!(target: "amf::Value::read_object", "inner object done={:?}", inner_done);
          }
        }
      }
      if marker == ObjectEnd { done = true}
    } // loop
    trace!(target: "amf::Value::read_object", "hash: {:?}", obj_hash);
    Ok(obj_hash)

  }


  pub async fn read<T>(mut reader: T) -> io::Result<Value>
  where T: AsyncRead + Unpin
  {
    let marker: u8 = reader.read_u8().await.expect("read AMF type marker");
    let value = match Marker::from_u8(marker) {
      Some(Utf8String) => {
        trace!(target: "amf::Value::read", "Utf8String");
        let s = Value::read_string(&mut reader).await.expect("read Amf0 Utf8String");
        Value::Utf8(s)
      },
      Some(Number) => {
        trace!(target: "amf::Value::read", "Number");
        let n = Value::read_number(&mut reader).await.expect("read Amf0 Number");
        Value::Number(n)
      },
      Some(Boolean) => {
        trace!(target: "amf::Value::read", "Boolean");
        let b = Value::read_bool(&mut reader).await.expect("read Amf0 Boolean");
        Value::Boolean(b)
      },
      Some(Object) => {
        trace!(target: "amf::Value::read", "Object");
        let hash = Value::read_object(&mut reader).await.expect("read Amf0 Object");
        Value::Object(hash)
      },
      Some(Null) => {
        trace!(target: "amf::Value::read", "Null");
        Value::Null
      },
      Some(EcmaArray) => {
        trace!(target: "amf::Value::read", "EcmaArray, skip 4 bytes then read object (!)");
        let fake_len = reader.read_u32().await.expect("ecma array length -- unused?");
        trace!(target: "amf::Value::read", "ignoring u32 {:?}", fake_len);
        let hash = Value::read_object(&mut reader).await.expect("read Amf0 Object");
        Value::Object(hash)
      },

      Some(t)   => panic!("umimplemented AMf0 type: {:?}", t),
      _         => panic!("unexpected AMf0 type: {}", marker)
    }; // match Marker
    info!(target: "amf::Value::read", "read value: {:?}", value);
    Ok(value)
  } // pub async fn read

  pub async fn write_string<T>(mut writer: T,
    s: String, write_marker: bool) -> io::Result<()>
    where T: AsyncWrite + Unpin
  {
    if write_marker {
      writer.write_u8(Utf8String as u8).await.expect("write String marker");
    }
    writer.write_u16(s.len() as u16).await.expect("write string length");
    writer.write_all(s.as_bytes()).await.expect("write_all string");
    Ok(())
  }

  pub async fn write_number<T>(mut writer: T,
    num: f64, write_marker: bool) -> io::Result<()>
    where T: AsyncWrite + Unpin
  {
    if write_marker {
      writer.write_u8(Number as u8).await.expect("write Number marker");
    }
    writer.write_all(&f64::to_be_bytes(num)).await.expect("write_all Number");
    Ok(())
  }

  async fn write_object<T>(mut writer: T,
                          hash: HashMap<String, Value>,
                          write_marker: bool) -> io::Result<()>
  where T: AsyncWrite + Unpin
  {
    if write_marker {
      writer.write_u8(Object as u8).await.expect("write Object marker");
    }
    let mut keys = Vec::new();
    for k in hash.keys() { keys.push(k) };
    keys.sort();

    for key in keys {
      // TODO: can this be a long string?
      Value::write_string(&mut writer, key.clone(), false).await.expect("write obj label"); // TODO: should not need to clone

      if let Some(value) = hash.get(key) {
        // TODO: cloning seems wrong here
        match value {
          Value::Utf8(s) => Value::write_string(&mut writer, s.clone(), true).await.expect("write string"),
          Value::Number(n) => Value::write_number(&mut writer, n.clone(), true).await.expect("write number"),
          _ => panic!("unimplemented nested value {:?}", value),
        }
      }
    }
    Value::write_string(&mut writer, "".to_string(), false).await.expect("write empty string to mark Object end");
    writer.write_u8(ObjectEnd as u8).await.expect("write ObjectEnd marker");
    Ok(())
  }

  pub async fn write<T>(mut writer: T, value: Value) -> io::Result<()>
  where T: AsyncWrite + Unpin
  {
    trace!(target: "amf::Value::write", "value: {:?}", value);

    match value {
      Value::Utf8(s) => {
        trace!(target: "amf::Value::write", "Utf8({:?})", s);
        // TODO check for long string
        Value::write_string(writer, s, true).await.expect("write string");
      },
      Value::Number(n) => {
        trace!(target: "amf::Value::write", "Number: {:?}", n);
        Value::write_number(writer, n, true).await.expect("write number");
      },
      Value::Boolean(b) => {
        trace!(target: "amf::Value::write", "Boolean: {:?}", b);
        writer.write_u8(Boolean as u8).await.expect("write Boolean marker");
        writer.write_u8(b as u8).await.expect("write_u8 Boolean");
     },
      Value::Object(o) => {
        trace!(target: "amf::Value::write", "Object: {:?}", o);
        Value::write_object(&mut writer, o, true).await.expect("read Amf0 Object");
      },
      Value::Null => {
        trace!(target: "amf::Value::write", "Null");
        writer.write_u8(Null as u8).await.expect("write Null marker");
      },
      // Value::EcmaArray => {
      //   trace!(target: "amf::Value::write", "EcmaArray, skip 4 bytes then read object (!)");
      //   let fake_len = reader.read_u32().await.expect("ecma array length -- unused?");
      //   trace!(target: "amf::Value::read", "ignoring u32 {:?}", fake_len);
      //   let hash = Value::read_object(&mut reader).await.expect("read Amf0 Object");
      //   Value::Object(hash)
      // },

      // _         => panic!("unexpected AMf0 type: {:?}", value)
    }; // match Marker
    Ok(())
  } // pub async fn read



} // impl Value

#[cfg(test)]
mod tests {
    use super::*;   // importing names from outer (for mod tests) scope.
    use crate::util::bytes_from_hex_string;

    #[tokio::test]
    async fn can_read_null() {
      let bytes = bytes_from_hex_string("05");
      let buf: &[u8] = &bytes;
      let value = Value::read(buf).await.expect("read");
      assert_eq!(value, Value::Null)
    }

    #[tokio::test]
    async fn can_write_null() {
      let expected = bytes_from_hex_string("05");
      let mut buf = Vec::new();
      Value::write(&mut buf, Value::Null).await.expect("write");
      assert_eq!(buf, expected);
    }


    // 02                       String type marker
    // 00 07                    7 bytes long
    // 63 6f 6e 6e 65 63 74     "connect"
    #[tokio::test]
    async fn can_read_string() {
      let bytes = bytes_from_hex_string("02 00 07 63 6f 6e 6e 65 63 74");
      let buf: &[u8] = &bytes;
      let value = Value::read(buf).await.expect("read");
      match value {
        Value::Utf8(s) => assert_eq!(s, "connect"),
                    _  => panic!("expected Utf8, got {:?}", value)
      }
    }

    #[tokio::test]
    async fn can_write_string() {
      let bytes = bytes_from_hex_string("02 00 07 63 6f 6e 6e 65 63 74");
      let expected: &[u8] = &bytes;

      let mut buf = Vec::new();
      Value::write(&mut buf, Value::Utf8("connect".to_string())).await.expect("write");
      assert_eq!(buf, expected);
    }


    #[tokio::test]
    async fn can_read_number_zero() {
      let num: f64 = 0.0;
      let byte_array: [u8; 9] = [0x00; 9];
      let buf: &[u8] = &byte_array;
      let value = Value::read(buf).await.expect("read");
      match value {
        Value::Number(val) => assert_eq!(val, num),
                        _  => panic!("expected Number, got {:?}", value)
      }
    }

    #[tokio::test]
    async fn can_write_number_zero() {
      let expected: [u8; 9] = [0x00; 9];
      let mut buf = Vec::new();
      Value::write(&mut buf, Value::Number(0.0)).await.expect("write");
      assert_eq!(buf, expected);
    }

    #[tokio::test]
    async fn can_read_bool_false() {
      let bytes = bytes_from_hex_string("01 00");
      let buf: &[u8] = &bytes;
      let value = Value::read(buf).await.expect("read");
      match value {
        Value::Boolean(s) => assert_eq!(s, false),
                        _  => panic!("expected Boolean, got {:?}", value)
      }
    }
    #[tokio::test]
    async fn can_read_bool_true() {
      let bytes = bytes_from_hex_string("01 01");
      let buf: &[u8] = &bytes;
      let value = Value::read(buf).await.expect("read");
      match value {
        Value::Boolean(s) => assert_eq!(s, true),
                        _  => panic!("expected Boolean, got {:?}", value)
      }
    }
    #[tokio::test]
    async fn can_write_bool_true() {
      let expected = bytes_from_hex_string("01 01");
      let mut buf = Vec::new();
      Value::write(&mut buf, Value::Boolean(true)).await.expect("write");
      assert_eq!(buf, expected);
    }

    #[tokio::test]
    async fn can_read_object_simple() {
     // 03                              Object marker
     //    00 06 66 6d  73 56 65 72                                 label: "fmsVer"
     //    02 00 0f 46 4d 53 2f 35 2c 30 2c 31  35 2c 35 30 30 34   value: Utf8(""FMS/5,0,15,5004"")
     //    00 0c 63 61 70 61 62 69 6c 69 74 69 65 73                label: "capabilities"
     //    00 40 6f e0 00 00 00 00 00                               value: Number(255.0)
     //    00 04 6d 6f 64 65                                        label: "mode"
     //    00 3f f0 00 00 00 00 00 00                               value: Number(1.0)
     //    00 00 09                     ObjectEnd

      let bytes = bytes_from_hex_string("
                  03
                  00 06 66 6d 73 56 65 72
                  02 00 0f 46 4d 53 2f 35 2c 30 2c 31  35 2c 35 30 30 34
                  00 0c 63 61 70 61 62 69 6c 69  74 69 65 73
                  00 40 6f e0 00 00 00 00 00
                  00 04 6d 6f 64 65
                  00 3f f0 00 00 00 00 00 00
                  00 00
                  09");

      let buf: &[u8] = &bytes;
      let value = Value::read(buf).await.expect("read");

      let mut expected = HashMap::new();
      expected.insert("capabilities".to_string(), Value::Number(255.0));
      expected.insert("fmsVer".to_string(), Value::Utf8("FMS/5,0,15,5004".to_string()));
      expected.insert("mode".to_string(), Value::Number(1.0));

      match value {
        Value::Object(h) => assert_eq!(h, expected),
                      _  => panic!("expected Object, got {:?}", value)
      }
    }

    #[tokio::test]
    async fn can_write_object_simple() {
    // 03                              Object marker
     //    00 0c 63 61 70 61 62 69 6c 69 74 69 65 73                label: "capabilities"
     //    00 40 6f e0 00 00 00 00 00                               value: Number(255.0)
     //    00 06 66 6d  73 56 65 72                                 label: "fmsVer"
     //    02 00 0f 46 4d 53 2f 35 2c 30 2c 31  35 2c 35 30 30 34   value: Utf8(""FMS/5,0,15,5004"")
     //    00 04 6d 6f 64 65                                        label: "mode"
     //    00 3f f0 00 00 00 00 00 00                               value: Number(1.0)
     //    00 00 09                     ObjectEnd

      let expected =  bytes_from_hex_string("
                      03
                      00 0c 63 61 70 61 62 69 6c 69  74 69 65 73
                      00 40 6f e0 00 00 00 00 00
                      00 06 66 6d 73 56 65 72
                      02 00 0f 46 4d 53 2f 35 2c 30 2c 31  35 2c 35 30 30 34
                      00 04 6d 6f 64 65
                      00 3f f0 00 00 00 00 00 00
                      00 00
                      09");

      let mut h = HashMap::new();
      h.insert("capabilities".to_string(), Value::Number(255.0));
      h.insert("fmsVer".to_string(), Value::Utf8("FMS/5,0,15,5004".to_string()));
      h.insert("mode".to_string(), Value::Number(1.0));

      let mut buf = Vec::new();
      Value::write(&mut buf, Value::Object(h)).await.expect("write");
      assert_eq!(buf, expected);
    }
} // mod tests