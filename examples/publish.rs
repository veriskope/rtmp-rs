extern crate pretty_env_logger;
use std::thread::sleep;
use std::time::Duration;
use url::Url;

fn main() {
  pretty_env_logger::init();

  let addr = "127.0.0.1:1935";

  let url = Url::parse(&format!("rtmp://{}/live/mystream", addr)).expect("url parse");

  let mut conn = rtmp::Connection::new(url);
  let stream = conn.new_stream();

  // optional set timeout to 1 sec: conn.set_timeout(1000);
  conn
    .connect_with_callback(move |response| {
      println!("===> connect response: {:?}", response);
      stream.publish("mystream".to_string(), rtmp::RecordFlag::Live);
      println!("===> published: {}", stream);
    })
    .expect("rtmp connect");
  println!("waiting");

  let some_time = Duration::from_millis(100);
  loop {
    sleep(some_time);
  }
  // let mut input = String::new();
  // println!("press return to quit");
  // std::io::stdin().read_line(&mut input).expect("stdio read_line");
}
