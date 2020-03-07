extern crate pretty_env_logger;
use std::thread::sleep;
use std::time::Duration;
use url::Url;

fn main() {
    let mut runtime = tokio::runtime::Runtime::new().expect("new Runtime");
    runtime.block_on(async {
        pretty_env_logger::init();

        let addr = "127.0.0.1:1935";

        let url = Url::parse(&format!("rtmp://{}/vod/media", addr)).expect("url parse");

        let mut conn = rtmp::Connection::new(url);
        // optional set timeout to 1 sec: conn.set_timeout(1000);
        let response = conn.connect().await.unwrap();
        println!("===> connect response: {:?}", response);
    });
}
