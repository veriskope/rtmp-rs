// use native_tls::TlsConnector;
// use tokio::net::TcpStream;
// use tokio_tls;
// use url::Url;

extern crate pretty_env_logger;

#[tokio::main]
async fn main() {
  pretty_env_logger::init();
  
  let _addr = "live-api-s.facebook.com:443";

  // let tcp = TcpStream::connect(addr).await?;
  // cannot use the `?` operator in a function that returns `()`
  // but it actually returns a result

  // let tcp = TcpStream::connect(addr).await.expect("tcp connection failed");
  // tcp.set_nodelay(true).expect("set_nodelay call failed");

  // let cx = TlsConnector::builder().build().expect("build TlsConnector");
  // let cx = tokio_tls::TlsConnector::from(cx);
  // let socket = cx.connect("live-api-s.facebook.com", tcp).await.expect("Tls connect");

  // let url = Url::parse(&format!("rtmps://{}:443/rtmp/", addr)).expect("url parse");

  // let mut conn = rtmp::Connection::new(url, socket);
  // // optional set timeout to 1 sec: conn.set_timeout(1000);  
  // conn.connect( |_| {}).await.expect("rtmp connection failed");

  let mut input = String::new();
  println!("press return to quit");
  std::io::stdin().read_line(&mut input).expect("stdio read_line");
}


