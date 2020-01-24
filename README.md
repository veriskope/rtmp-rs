## Running the example

with logging...
```
RUST_LOG=trace  cargo run --example connect
```

## Work in progress

Thinking that an API that looks like this would be nice

```
#[tokio::main]
async fn main() {

  let url = "rtmp://localhost/live"
  let conn = rtmp::Connection::new(url).await?;
  // optional set timeout to 1 sec: conn.set_timeout(1000);  
  conn.connect().await?;
  let result = conn.send_command("get_user_info", ["fred"]).await?
  match result {
    Err(e) => println!("command failed", e),
    Ok(info) => println!("Got info: {}", info)
  }
}
```