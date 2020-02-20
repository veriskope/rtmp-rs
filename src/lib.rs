// used in amf.rs
#[macro_use]
extern crate enum_primitive_derive;

pub mod amf;
pub mod error;

mod chunk;
pub use chunk::Message;

mod stream;
pub use stream::NetStream;
pub use stream::RecordFlag;

mod connection;
pub use connection::Connection;

mod util;
