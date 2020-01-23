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
    
    

        Ok(())
    }
  
}

