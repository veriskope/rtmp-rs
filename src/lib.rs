#![warn(missing_debug_implementations, missing_copy_implementations)]

// used in amf.rs
#[macro_use]
extern crate enum_primitive_derive;

pub mod amf;
pub mod error;

mod message;
pub use message::Message;

mod stream;
pub use stream::NetStream;
pub use stream::RecordFlag;

mod connection;
pub use connection::Connection;

mod chunk;
mod util;
