extern crate pretty_env_logger;

use tokio::net::TcpStream;
use url::Url;

#[tokio::main]
async fn main() {
  pretty_env_logger::init();

  let addr = "127.0.0.1:1935";
  // let tcp = TcpStream::connect(addr).await?;
  // cannot use the `?` operator in a function that returns `()`
  // but it actually returns a result

  let tcp = TcpStream::connect(addr).await.expect("tcp connection failed");
  tcp.set_nodelay(true).expect("set_nodelay call failed");

  let url = Url::parse(&format!("rtmp://{}/vod/media", addr)).expect("url parse");

  let mut conn = rtmp::Connection::new(url, tcp);
  // optional set timeout to 1 sec: conn.set_timeout(1000);  
  conn.connect().await.expect("rtmp connection failed");

  let mut input = String::new();
  println!("press return to quit");
  std::io::stdin().read_line(&mut input).expect("stdio read_line");
}


