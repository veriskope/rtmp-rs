extern crate pretty_env_logger;
use rtmp::{MessageResponse, NetStream, RecordFlag};
use std::thread::sleep;
use std::time::Duration;
use url::Url;

fn stream_callback(stream: &NetStream, msg: MessageResponse) {
    println!(
        "===================> stream_callback: {:#?} {:#?}",
        stream, msg
    );
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:1935";

    let url = Url::parse(&format!("rtmp://{}/live/mystream", addr)).expect("url parse");

    let mut conn = rtmp::Connection::new(url);

    // optional set timeout to 1 sec: conn.set_timeout(1000);
    conn.connect_with_callback(move |mut cn, response| {
        println!("===> connect response: {:?}", response);
        tokio::spawn(async move {
            // let (maybe_stream, message) = match cn.new_stream().await {
            //     Ok((stream, message)) => (Some(stream), message),
            //     Err(err_message) => (None, err_message.0),
            // };
            if let Ok((mut stream, message)) = cn.new_stream().await {
                println!("===> created stream: {:?}", stream);
                stream_callback(&stream, message);
                stream
                    .publish("cameraFeed", RecordFlag::Live)
                    .await
                    .unwrap();
                println!("===> published stream: {:?}", stream);
            }

            // normal rust
            // let (stream, _) = cn.new_stream().await.expect("new stream");
            // stream.publish("cameraFeed", RecordFlag::Live).await;
        });
        // TODO hack: publish called automatically on successful create
        // stream.publish("mystream".to_string(), rtmp::RecordFlag::Live).await;
        // println!("===> published: {:?}", stream);
    })
    .expect("rtmp connect");
    println!("waiting");

    let some_time = Duration::from_millis(100);
    loop {
        sleep(some_time);
    }
}
