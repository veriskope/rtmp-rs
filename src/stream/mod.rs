mod flag;
pub use flag::RecordFlag;
use std::sync::RwLock;
use std::fmt;

pub struct NetStreamInfo {
  name: String,
  flag: RecordFlag,
}


pub struct NetStream {
  id: u32,  // immutable, TODO: maybe stream should generate its own id?
  // publish happens rarely, typically once per stream
  // so RwLock allows for many readers
  info: RwLock<Option<NetStreamInfo>>
}

impl NetStream {
  pub fn new(id: u32) -> Self {
    NetStream {
      id,
      info: RwLock::new(None),
    }
  }

  pub fn publish(&self, name: String, flag: RecordFlag) {
    let mut info = self.info.write().unwrap();
    *info = Some( NetStreamInfo { name, flag } );
  }

}

impl fmt::Display for NetStream {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let info_ref = self.info.read().unwrap();
    match &*info_ref {
      None => write!(f, "NetStream id{}: not published)", self.id),
      Some(info) =>  write!(f, "NetStream id{}: {} {})", self.id, info.name, info.flag)
    }
  }
}
