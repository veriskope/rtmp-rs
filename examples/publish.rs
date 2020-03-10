extern crate pretty_env_logger;
use futures::stream::StreamExt;
use rtmp::RecordFlag;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let addr = "127.0.0.1:1935";

    let url = Url::parse(&format!("rtmp://{}/live/mystream", addr))?;

    let mut cn = rtmp::Connection::new(url);

    let response = cn.connect().await?;
    println!("===> connect response: {:?}", response);

    // let (maybe_stream, message) = match cn.new_stream().await {
    //     Err(err_message) => (None, err_message.0),
    //     Ok((stream, message)) => (Some(stream), message),
    // };
    let (mut stream, _message) = cn.new_stream().await?;
    println!("===> created stream: {:?}", stream);
    stream.publish("cameraFeed", RecordFlag::Live).await?;
    println!("===> published stream: {:?}", stream);

    while let Some(status) = stream.next().await {
        println!("===> {:#?}", status)
    }

    // normal rust
    // let (stream, _) = cn.new_stream().await.expect("new stream");
    // stream.publish("cameraFeed", RecordFlag::Live).await;
    Ok(())
}
