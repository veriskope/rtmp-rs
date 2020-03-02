use tokio::prelude::*;
extern crate proc_macro;

extern crate num_traits;
use num_traits::FromPrimitive;

#[derive(Debug, PartialEq, Primitive)]
pub enum EventType {
    StreamBegin = 0,
    StreamEOF = 1,
    StreamDry = 2,
    SetBufferLength = 3,
    StreamIsRecorded = 4,
    // what is 5?
    PingRequest = 6,
    PingResponse = 7,
}

#[derive(Debug, PartialEq)]
pub enum Event {
    StreamBegin(u32), // stream_id
                      // StreamEOF(u32),            // stream_id
                      // StreamDry(u32),            // stream_id
                      // SetBufferLength(u32, u32), // stream_id, buffer length in ms
                      // StreamIsRecorded(u32),     // stream_id
                      // PingRequest(u32),          // timestamp
                      // PingResponse(u32),         // timestamp
}

// TODO: can we just derive Read on these, given that we know type?
#[derive(Debug, PartialEq)]
pub enum Signal {
    SetChunkSize(u32), // 31 bits actually
    Abort(u32),
    AckChunk(u32),
    UserControlMessage(Event),
    SetWindowAckSize(u32),
    SetPeerBandwidth(u32, u8),
}

// the table of constants could be merged with Enum declaration with
// https://github.com/rust-lang/rust/issues/60553
#[derive(Debug, PartialEq, Primitive)]
pub enum SignalType {
    SetChunkSize = 01,
    Abort = 02,
    AckChunk = 03,
    UserControlMessage = 04,
    SetWindowAckSize = 05,
    SetPeerBandwidth = 06,
}
use SignalType::*;

impl Signal {
    // there must be a better way, or put this somewhere else
    // prolly want to inline to
    async fn read_le_stream_id<T>(mut reader: T) -> io::Result<u32>
    where
        T: AsyncRead + Unpin,
    {
        let mut buf: [u8; 4] = [0_u8; 4];
        reader.read_exact(&mut buf).await.expect("4 byte stream id");
        Ok(u32::from_le_bytes(buf))
    }

    async fn read_user_control_message<T>(mut reader: T) -> io::Result<Signal>
    where
        T: AsyncRead + Unpin,
    {
        let event_type = reader.read_u16().await.expect("read event type");
        let event: Event = match EventType::from_u16(event_type) {
        Some(EventType::StreamBegin) => {
          let stream_id = Signal::read_le_stream_id(reader).await.unwrap();
          Event::StreamBegin(stream_id)
        },
        _ => {
          panic!("unimplemented event_type {:?}, reading user control message", event_type);
        }
        // StreamEOF = 1,
        // StreamDry = 2,
        // SetBufferLength = 3,
        // StreamIsRecorded = 4,
        // // what is 5?
        // PingRequest = 6,
        // PingResponse = 7,
    };

        Ok(Signal::UserControlMessage(event))
    }
    pub async fn read<T>(mut reader: T, chunk_type: u8) -> io::Result<Signal>
    where
        T: AsyncRead + Unpin,
    {
        let signal: Signal = match SignalType::from_u8(chunk_type) {
            // TODO: how to match SignalType
            Some(SetChunkSize) => {
                let size = reader.read_u32().await?;
                Signal::SetChunkSize(size)
            }
            Some(Abort) => {
                let data = reader.read_u32().await?;
                Signal::Abort(data)
            }
            Some(AckChunk) => {
                let data = reader.read_u32().await?;
                Signal::AckChunk(data)
            }
            Some(UserControlMessage) => Signal::read_user_control_message(reader)
                .await
                .expect("read control msg"),
            Some(SetWindowAckSize) => {
                let window_size = reader.read_u32().await?;
                Signal::SetWindowAckSize(window_size)
            }
            Some(SetPeerBandwidth) => {
                let window_size = reader.read_u32().await?;
                let limit_type = reader.read_u8().await?; // TODO: make enum
                Signal::SetPeerBandwidth(window_size, limit_type)
            }
            _ => panic!("unimplemented read for signal chunk type {}", chunk_type),
        };
        Ok(signal)
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[tokio::test]
    async fn can_read_abort() {
        use crate::util::bytes_from_hex_string;
        let bytes = bytes_from_hex_string("00 00 00 01");
        let buf: &[u8] = &bytes;
        let chunk = (Signal::read(buf, Abort as u8).await).expect("read");
        assert_eq!(chunk, Signal::Abort(1));
    }

    // TODO: reading SetChunkSize should fail/warn if 32 bits, must be 31 bit value

    #[tokio::test]
    async fn can_read_set_chunk_size() {
        use crate::util::bytes_from_hex_string;
        let bytes = bytes_from_hex_string("00 00 00 ff");

        let buf: &[u8] = &bytes;
        let chunk = (Signal::read(buf, SetChunkSize as u8).await).expect("read");
        assert_eq!(chunk, Signal::SetChunkSize(255));
    }

    #[tokio::test]
    async fn can_read_ack_chunk() {
        use crate::util::bytes_from_hex_string;
        let bytes = bytes_from_hex_string("00 26 25 a0");

        let buf: &[u8] = &bytes;
        let chunk = (Signal::read(buf, AckChunk as u8).await).expect("read");
        assert_eq!(chunk, Signal::AckChunk(2500000));
    }

    #[tokio::test]
    async fn can_read_user_control_message() {
        use crate::util::bytes_from_hex_string;
        let bytes = bytes_from_hex_string("00 00 01 00 00 00");

        let buf: &[u8] = &bytes;
        let chunk = (Signal::read(buf, UserControlMessage as u8).await).expect("read");
        assert_eq!(chunk, Signal::UserControlMessage(Event::StreamBegin(1)));
    }

    #[tokio::test]
    async fn can_read_set_window_ack_size() {
        use crate::util::bytes_from_hex_string;
        let bytes = bytes_from_hex_string("00 26 25 a0");

        let buf: &[u8] = &bytes;
        let chunk = (Signal::read(buf, SetWindowAckSize as u8).await).expect("read");
        assert_eq!(chunk, Signal::SetWindowAckSize(2500000));
    }

    #[tokio::test]
    async fn can_read_set_peer_bandwidth() {
        use crate::util::bytes_from_hex_string;
        let bytes = bytes_from_hex_string("00 26 25 a0 02");

        let buf: &[u8] = &bytes;
        let chunk = (Signal::read(buf, SetPeerBandwidth as u8).await).expect("read");
        assert_eq!(chunk, Signal::SetPeerBandwidth(2500000, 2));
    }
} // mod tests
