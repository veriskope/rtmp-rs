## Running the example

requires unstable
```
cargo +nightly build
```

with logging...
```
RUST_LOG=trace  cargo +nightly run --example connect
```

run tests with logging:
```
RUST_LOG=trace  cargo +nightly test
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

## TODO

- I've gotten a bit fast-and-loose with adding crates, would like to consider
  reducing dependencies once the basics are working
- maybe use procedural macro to implement reading/writing of chunks with less 
  boilerplate code