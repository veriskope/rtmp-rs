extern crate pretty_env_logger;
use rtmp::{MessageResponse, NetStream, RecordFlag};
use url::Url;

fn stream_callback(stream: &NetStream, msg: MessageResponse) {
    println!("===> stream_callback: {:#?} {:#?}", stream, msg);
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:1935";

    let url = Url::parse(&format!("rtmp://{}/live/mystream", addr)).expect("url parse");

    let mut cn = rtmp::Connection::new(url);

    let response = cn.connect().await.unwrap();
    println!("===> connect response: {:?}", response);

    // let (maybe_stream, message) = match cn.new_stream().await {
    //     Err(err_message) => (None, err_message.0),
    //     Ok((stream, message)) => (Some(stream), message),
    // };
    let (mut stream, message) = cn.new_stream().await.unwrap();
    println!("===> created stream: {:?}", stream);
    stream_callback(&stream, message);
    stream
        .publish("cameraFeed", RecordFlag::Live)
        .await
        .unwrap();
    println!("===> published stream: {:?}", stream);

    // normal rust
    // let (stream, _) = cn.new_stream().await.expect("new stream");
    // stream.publish("cameraFeed", RecordFlag::Live).await;
}
