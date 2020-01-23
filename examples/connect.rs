use tokio::net::TcpStream;

#[tokio::main]
async fn main() {
  let addr = "127.0.0.1:1234";
  // let tcp = TcpStream::connect(addr).await?;
  // cannot use the `?` operator in a function that returns `()`
  // but it actually returns a result

  let tcp = TcpStream::connect(addr).await.expect("tcp connection failed");

  let url = format!("rtmp://{}/live", addr);
  let mut conn = rtmp::Connection::new(url, tcp);
  // optional set timeout to 1 sec: conn.set_timeout(1000);  
  conn.connect().await.expect("rtmp connection failed");

}


