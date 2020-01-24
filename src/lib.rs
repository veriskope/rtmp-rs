use tokio::prelude::*;
use tokio::{io::BufReader, net::TcpStream};
pub mod error;
mod handshake;
use handshake::{HandshakeProcessResult, Handshake, PeerType};
use log::{info, warn};
use rml_amf0::{Amf0Value, serialize, deserialize};
use std::collections::HashMap;

pub struct Connection<T = TcpStream> {
    url: String,
    cn: BufReader<T>
}

impl<Transport: AsyncRead + AsyncWrite + Unpin> Connection<Transport> {
    // consider passing ConnectionOptions with transport? and (URL | parts)
    pub fn new(url: String, transport: Transport) -> Self {
      info!(target: "rtmp::Connection", "new");
      Connection {
        url,
        cn: BufReader::new(transport),
      }
    }

    // TODO: param => params: &[Amf0Value]
    pub async fn send_command(&mut self, command_name: String, param: Amf0Value) -> Result<(), Box<dyn std::error::Error>> {
        info!(target: "rtmp::Connection", "send_command");

        let transaction_id:f64 = 1.0;   // connect message is always 1, I think we increment from here
        let mut values = vec![
            Amf0Value::Utf8String(command_name),
            Amf0Value::Number(transaction_id),
            param       // connect has one param, should allow array
        ];
    
        let bytes = rml_amf0::serialize(&values).expect("serialize command data");
        let bytes_written = self.cn.write(&bytes).await.expect("write Amf command");
        if bytes_written == 0 {
            panic!("connection unexpectedly closed");   // TODO: return real error
        } else {
            info!(target: "rtmp::Connection", "wrote {} bytes", bytes_written);
        }
        self.cn.flush().await.expect("send_command: flush after write");

        let mut read_buffer = [0_u8; 1024];     // TODO: use Bytes?
        let num_bytes = self.cn.read(&mut read_buffer).await.expect("read command result");
        if num_bytes == 0 {
            panic!("connection unexpectedly closed");   // TODO: return real error
        } else {
            info!(target: "rtmp::Connection", "read {} bytes", num_bytes);
        }
        Ok(())
    }

    //pub async fn connect(&mut self) -> Result<(), Error> {
    // error[E0412]: cannot find type `Error` in this scope
    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {

        let mut handshake = Handshake::new(PeerType::Client);
        let c0_and_c1 = handshake.generate_outbound_p0_and_p1().unwrap();
        self.cn.write(&c0_and_c1).await.expect("write c0_and_c1");
        
        let mut read_buffer = [0_u8; 1024];
        loop {
            let num_bytes = self.cn.read(&mut read_buffer).await.expect("read");
            if num_bytes == 0 {
                panic!("connection unexpectedly closed");   // TODO: return real error
            }
            println!("bytes read: {}", num_bytes);
                let (is_finished, response_bytes) = match handshake.process_bytes(&read_buffer) {
                Err(x) => panic!("Error returned: {:?}", x),
                Ok(HandshakeProcessResult::InProgress {response_bytes: bytes}) => (false, bytes),
                Ok(HandshakeProcessResult::Completed {response_bytes: bytes, remaining_bytes: _}) => (true, bytes)
            };
            if response_bytes.len() > 0 {
                self.cn.write(&response_bytes).await.unwrap(); // TODO: return error??
            }
            if is_finished { 
                break; 
            }
        }
        println!("getting ready to send connect command");
        let mut properties = HashMap::new();
        //TODO -- why send "app" here?
        //properties.insert("app".to_string(), Amf0Value::Utf8String(app_name));
        let flash_version = "WIN 23,0,0,207".to_string();   // TODO: must we, really?
        properties.insert("flashVer".to_string(), Amf0Value::Utf8String(flash_version));
        properties.insert("objectEncoding".to_string(), Amf0Value::Number(0.0));
        properties.insert("tcUrl".to_string(), Amf0Value::Utf8String("rtmp://localhost:1935/live".to_string()));

        //let params = [Amf0Value::Object(properties)];
        // TODO: Amf0Value - should implement Copy, & API shouldn't need copy
        let result = self.send_command("connect".to_string(), Amf0Value::Object(properties)).await;
        println!("send_command result: {:?}", result);
        result
    }
  


}

