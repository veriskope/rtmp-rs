use tokio::prelude::*;
use tokio::{io::BufReader, net::TcpStream};
pub mod error;


pub struct Connection<T = TcpStream> {
    url: String,
    bufconn: BufReader<T>
}

impl<Transport: AsyncRead + AsyncWrite + Unpin> Connection<Transport> {
    // consider passing ConnectionOptions with transport? and (URL | parts)
    pub fn new(url: String, transport: Transport) -> Self {
      Connection {
        url,
        bufconn: BufReader::new(transport),
      }
    }
    //pub async fn connect(&mut self) -> Result<(), Error> {
    // error[E0412]: cannot find type `Error` in this scope
    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("TODO -- implement connect");
        Ok(())
    }
  
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
