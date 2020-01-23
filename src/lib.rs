use tokio::prelude::*;
use tokio::{io::BufReader, net::TcpStream};
pub mod error;
mod handshake;
use handshake::{HandshakeProcessResult, Handshake, PeerType};

pub struct Connection<T = TcpStream> {
    url: String,
    cn: BufReader<T>
}

impl<Transport: AsyncRead + AsyncWrite + Unpin> Connection<Transport> {
    // consider passing ConnectionOptions with transport? and (URL | parts)
    pub fn new(url: String, transport: Transport) -> Self {
      Connection {
        url,
        cn: BufReader::new(transport),
      }
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

        Ok(())
    }
  
}

