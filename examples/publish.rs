extern crate pretty_env_logger;
use std::thread::sleep;
use std::time::Duration;
use url::Url;

fn stream_callback(stream: rtmp::NetStream, msg: rtmp::Message) {
    println!(
        "===================> stream_callback: {:?} {:?}",
        stream, msg
    );
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:1935";

    let url = Url::parse(&format!("rtmp://{}/live/mystream", addr)).expect("url parse");

    let mut conn = rtmp::Connection::new(url, Some(stream_callback));

    // optional set timeout to 1 sec: conn.set_timeout(1000);
    conn.connect_with_callback(move |mut cn, response| {
        println!("===> connect response: {:?}", response);
        let stream = cn.new_stream();
        println!("===> created stream: {:?}", stream);
        // TODO hack: publish called automatically on successful create
        // stream.publish("mystream".to_string(), rtmp::RecordFlag::Live);
        // println!("===> published: {:?}", stream);
    })
    .expect("rtmp connect");
    println!("waiting");

    let some_time = Duration::from_millis(100);
    loop {
        sleep(some_time);
    }
}
