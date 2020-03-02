use log::{info, warn};
use std::ops::{Deref, DerefMut};
use tokio::prelude::*;

// wrapper for TCPStream with helpful function

pub struct BufReadWriter<T: AsyncWrite + Unpin> {
    pub buf: T,
}

impl<T> Deref for BufReadWriter<T>
where
    T: AsyncWrite + Unpin,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.buf
    }
}

impl<T> DerefMut for BufReadWriter<T>
where
    T: AsyncWrite + Unpin,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buf
    }
}

impl<T> BufReadWriter<T>
where
    T: AsyncWrite + Unpin,
{
    pub fn new(buf: T) -> Self {
        Self { buf }
    }
    /// useful function with error if not all bytes can be written
    pub async fn write_exact(&mut self, bytes: &[u8]) -> io::Result<usize>
    where
        T: AsyncWrite + Unpin,
    {
        let bytes_written = self.buf.write(bytes).await.expect("write");
        if bytes_written == 0 {
            warn!(target: "bufreadwriter", "connection unexpectedly closed");
            panic!("connection unexpectedly closed"); // TODO: return real error
        } else {
            info!(target: "bufreadwriter", "wrote {} bytes", bytes_written);
        }
        self.buf.flush().await.expect("flush after write"); // TODO: needed?
        return Ok(bytes_written);
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[tokio::test]
    async fn can_write() {
        let mut buf = BufReadWriter::new(Vec::new());
        let bytes = b"hello";
        let bytes_written = buf.write(bytes).await.expect("write bytes");
        assert_eq!(bytes_written, bytes.len());
        //Ok(bytes.len())
    }
    // #[tokio::test]
    // async fn error_writing_too_much() {
    //   let mut four_byte_buffer:[u8; 4] = [0u8; 4];
    //   let mut buf = BufReadWriter::new(&mut four_byte_buffer);
    //   let bytes = b"hello";
    //   let bytes_written = buf.write(bytes).await.expect("write bytes");
    //   assert_eq!(bytes_written, 4); maybe this is 0, doesn't matter for my purposes
    // }
}
