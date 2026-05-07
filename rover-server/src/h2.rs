use bytes::{Buf, BufMut, Bytes, BytesMut};
use hpack::{Decoder, Encoder};

pub const CLIENT_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
pub const DEFAULT_INITIAL_WINDOW_SIZE: i32 = 65_535;
pub const SETTINGS_ENABLE_CONNECT_PROTOCOL: u16 = 0x8;
pub const FLAG_ACK: u8 = 0x1;
pub const FLAG_END_STREAM: u8 = 0x1;
pub const FLAG_END_HEADERS: u8 = 0x4;
pub const ERROR_NO_ERROR: u32 = 0x0;
pub const ERROR_PROTOCOL_ERROR: u32 = 0x1;
pub const ERROR_INTERNAL_ERROR: u32 = 0x2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    Data = 0x0,
    Headers = 0x1,
    RstStream = 0x3,
    Settings = 0x4,
    Ping = 0x6,
    Goaway = 0x7,
    WindowUpdate = 0x8,
}

impl FrameType {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x0 => Some(Self::Data),
            0x1 => Some(Self::Headers),
            0x3 => Some(Self::RstStream),
            0x4 => Some(Self::Settings),
            0x6 => Some(Self::Ping),
            0x7 => Some(Self::Goaway),
            0x8 => Some(Self::WindowUpdate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub frame_type: FrameType,
    pub flags: u8,
    pub stream_id: u32,
    pub payload: Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum H2CodecError {
    FrameTooLarge(usize),
    InvalidStreamId(u32),
    UnknownFrameType(u8),
    InvalidPayload(&'static str),
}

pub struct HpackCodec {
    decoder: Decoder<'static>,
    encoder: Encoder<'static>,
}

impl HpackCodec {
    pub fn new() -> Self {
        Self {
            decoder: Decoder::new(),
            encoder: Encoder::new(),
        }
    }

    pub fn decode(&mut self, block: &[u8]) -> Result<Vec<(String, String)>, H2CodecError> {
        self.decoder
            .decode(block)
            .map_err(|_| H2CodecError::InvalidPayload("invalid HPACK header block"))?
            .into_iter()
            .map(|(name, value)| {
                let name = String::from_utf8(name)
                    .map_err(|_| H2CodecError::InvalidPayload("invalid header name utf8"))?;
                let value = String::from_utf8(value)
                    .map_err(|_| H2CodecError::InvalidPayload("invalid header value utf8"))?;
                Ok((name, value))
            })
            .collect()
    }

    pub fn encode(&mut self, headers: &[(&[u8], &[u8])]) -> Bytes {
        Bytes::from(self.encoder.encode(headers.iter().copied()))
    }
}

impl Default for HpackCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for H2CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FrameTooLarge(len) => write!(f, "h2 frame too large: {len}"),
            Self::InvalidStreamId(id) => write!(f, "invalid h2 stream id: {id}"),
            Self::UnknownFrameType(kind) => write!(f, "unknown h2 frame type: {kind}"),
            Self::InvalidPayload(message) => write!(f, "invalid h2 payload: {message}"),
        }
    }
}

impl std::error::Error for H2CodecError {}

impl Frame {
    pub fn is_ack(&self) -> bool {
        self.flags & FLAG_ACK != 0
    }

    pub fn is_end_stream(&self) -> bool {
        self.flags & FLAG_END_STREAM != 0
    }

    pub fn is_end_headers(&self) -> bool {
        self.flags & FLAG_END_HEADERS != 0
    }
}

pub fn encode_frame(frame: &Frame, out: &mut impl BufMut) -> Result<(), H2CodecError> {
    let len = frame.payload.len();
    if len > 0x00ff_ffff {
        return Err(H2CodecError::FrameTooLarge(len));
    }
    if frame.stream_id & 0x8000_0000 != 0 {
        return Err(H2CodecError::InvalidStreamId(frame.stream_id));
    }

    out.put_u8(((len >> 16) & 0xff) as u8);
    out.put_u8(((len >> 8) & 0xff) as u8);
    out.put_u8((len & 0xff) as u8);
    out.put_u8(frame.frame_type as u8);
    out.put_u8(frame.flags);
    out.put_u32(frame.stream_id & 0x7fff_ffff);
    out.put_slice(&frame.payload);
    Ok(())
}

pub fn decode_frame(input: &mut BytesMut) -> Result<Option<Frame>, H2CodecError> {
    if input.len() < 9 {
        return Ok(None);
    }

    let len = ((input[0] as usize) << 16) | ((input[1] as usize) << 8) | input[2] as usize;
    if input.len() < 9 + len {
        return Ok(None);
    }

    let frame_type =
        FrameType::from_u8(input[3]).ok_or(H2CodecError::UnknownFrameType(input[3]))?;
    let flags = input[4];
    let stream_id = u32::from_be_bytes([input[5], input[6], input[7], input[8]]) & 0x7fff_ffff;

    input.advance(9);
    let payload = input.split_to(len).freeze();

    Ok(Some(Frame {
        frame_type,
        flags,
        stream_id,
        payload,
    }))
}

pub fn settings_frame(settings: &[(u16, u32)]) -> Frame {
    let mut payload = BytesMut::with_capacity(settings.len() * 6);
    for (id, value) in settings {
        payload.put_u16(*id);
        payload.put_u32(*value);
    }
    Frame {
        frame_type: FrameType::Settings,
        flags: 0,
        stream_id: 0,
        payload: payload.freeze(),
    }
}

pub fn decode_settings(frame: &Frame) -> Result<Vec<(u16, u32)>, H2CodecError> {
    if frame.frame_type != FrameType::Settings {
        return Err(H2CodecError::InvalidPayload("expected SETTINGS frame"));
    }
    if frame.stream_id != 0 {
        return Err(H2CodecError::InvalidPayload("SETTINGS stream id must be 0"));
    }
    if frame.is_ack() {
        if !frame.payload.is_empty() {
            return Err(H2CodecError::InvalidPayload(
                "SETTINGS ack payload must be empty",
            ));
        }
        return Ok(Vec::new());
    }
    if frame.payload.len() % 6 != 0 {
        return Err(H2CodecError::InvalidPayload(
            "SETTINGS payload must be a multiple of 6",
        ));
    }

    let mut payload = frame.payload.clone();
    let mut settings = Vec::with_capacity(payload.len() / 6);
    while payload.has_remaining() {
        settings.push((payload.get_u16(), payload.get_u32()));
    }
    Ok(settings)
}

pub fn settings_ack_frame() -> Frame {
    Frame {
        frame_type: FrameType::Settings,
        flags: 0x1,
        stream_id: 0,
        payload: Bytes::new(),
    }
}

pub fn ping_frame(opaque: [u8; 8], ack: bool) -> Frame {
    Frame {
        frame_type: FrameType::Ping,
        flags: if ack { 0x1 } else { 0 },
        stream_id: 0,
        payload: Bytes::copy_from_slice(&opaque),
    }
}

pub fn decode_ping(frame: &Frame) -> Result<[u8; 8], H2CodecError> {
    if frame.frame_type != FrameType::Ping {
        return Err(H2CodecError::InvalidPayload("expected PING frame"));
    }
    if frame.stream_id != 0 {
        return Err(H2CodecError::InvalidPayload("PING stream id must be 0"));
    }
    if frame.payload.len() != 8 {
        return Err(H2CodecError::InvalidPayload("PING payload must be 8 bytes"));
    }

    let mut opaque = [0; 8];
    opaque.copy_from_slice(&frame.payload);
    Ok(opaque)
}

pub fn window_update_frame(stream_id: u32, increment: u32) -> Result<Frame, H2CodecError> {
    if increment == 0 || increment > 0x7fff_ffff {
        return Err(H2CodecError::InvalidPayload(
            "window increment must be 1..=2^31-1",
        ));
    }
    let mut payload = BytesMut::with_capacity(4);
    payload.put_u32(increment & 0x7fff_ffff);
    Ok(Frame {
        frame_type: FrameType::WindowUpdate,
        flags: 0,
        stream_id,
        payload: payload.freeze(),
    })
}

pub fn decode_window_update(frame: &Frame) -> Result<u32, H2CodecError> {
    if frame.frame_type != FrameType::WindowUpdate {
        return Err(H2CodecError::InvalidPayload("expected WINDOW_UPDATE frame"));
    }
    if frame.payload.len() != 4 {
        return Err(H2CodecError::InvalidPayload(
            "WINDOW_UPDATE payload must be 4 bytes",
        ));
    }

    let mut payload = frame.payload.clone();
    let increment = payload.get_u32() & 0x7fff_ffff;
    if increment == 0 {
        return Err(H2CodecError::InvalidPayload(
            "window increment must be non-zero",
        ));
    }
    Ok(increment)
}

pub fn rst_stream_frame(stream_id: u32, error_code: u32) -> Frame {
    let mut payload = BytesMut::with_capacity(4);
    payload.put_u32(error_code);
    Frame {
        frame_type: FrameType::RstStream,
        flags: 0,
        stream_id,
        payload: payload.freeze(),
    }
}

pub fn goaway_frame(last_stream_id: u32, error_code: u32) -> Frame {
    let mut payload = BytesMut::with_capacity(8);
    payload.put_u32(last_stream_id & 0x7fff_ffff);
    payload.put_u32(error_code);
    Frame {
        frame_type: FrameType::Goaway,
        flags: 0,
        stream_id: 0,
        payload: payload.freeze(),
    }
}

pub fn decode_goaway(frame: &Frame) -> Result<(u32, u32), H2CodecError> {
    if frame.frame_type != FrameType::Goaway {
        return Err(H2CodecError::InvalidPayload("expected GOAWAY frame"));
    }
    if frame.stream_id != 0 {
        return Err(H2CodecError::InvalidPayload("GOAWAY stream id must be 0"));
    }
    if frame.payload.len() < 8 {
        return Err(H2CodecError::InvalidPayload(
            "GOAWAY payload must be at least 8 bytes",
        ));
    }

    let mut payload = frame.payload.clone();
    Ok((payload.get_u32() & 0x7fff_ffff, payload.get_u32()))
}

pub fn data_frame(stream_id: u32, data: Bytes, end_stream: bool) -> Frame {
    Frame {
        frame_type: FrameType::Data,
        flags: if end_stream { 0x1 } else { 0 },
        stream_id,
        payload: data,
    }
}

pub fn headers_frame(
    stream_id: u32,
    header_block: Bytes,
    end_headers: bool,
    end_stream: bool,
) -> Frame {
    let mut flags = 0;
    if end_stream {
        flags |= FLAG_END_STREAM;
    }
    if end_headers {
        flags |= FLAG_END_HEADERS;
    }
    Frame {
        frame_type: FrameType::Headers,
        flags,
        stream_id,
        payload: header_block,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_round_trip_data_frame() {
        let frame = data_frame(1, Bytes::from_static(b"hello"), true);
        let mut encoded = BytesMut::new();
        encode_frame(&frame, &mut encoded).expect("encode");

        let decoded = decode_frame(&mut encoded).expect("decode").expect("frame");
        assert_eq!(decoded, frame);
        assert!(encoded.is_empty());
    }

    #[test]
    fn should_wait_for_complete_frame() {
        let frame = ping_frame(*b"12345678", false);
        let mut encoded = BytesMut::new();
        encode_frame(&frame, &mut encoded).expect("encode");
        encoded.truncate(12);

        assert_eq!(decode_frame(&mut encoded).expect("decode"), None);
    }

    #[test]
    fn should_encode_settings_for_extended_connect() {
        let frame = settings_frame(&[(SETTINGS_ENABLE_CONNECT_PROTOCOL, 1)]);
        assert_eq!(frame.frame_type, FrameType::Settings);
        assert_eq!(frame.stream_id, 0);
        assert_eq!(frame.payload.as_ref(), &[0, 8, 0, 0, 0, 1]);
        assert_eq!(decode_settings(&frame).expect("settings"), vec![(0x8, 1)]);
    }

    #[test]
    fn should_validate_settings_ack_payload() {
        let frame = Frame {
            frame_type: FrameType::Settings,
            flags: FLAG_ACK,
            stream_id: 0,
            payload: Bytes::from_static(b"bad"),
        };

        assert!(matches!(
            decode_settings(&frame),
            Err(H2CodecError::InvalidPayload(_))
        ));
    }

    #[test]
    fn should_decode_ping_payload() {
        let frame = ping_frame(*b"12345678", true);
        assert!(frame.is_ack());
        assert_eq!(decode_ping(&frame).expect("ping"), *b"12345678");
    }

    #[test]
    fn should_reject_invalid_window_increment() {
        assert!(matches!(
            window_update_frame(0, 0),
            Err(H2CodecError::InvalidPayload(_))
        ));
    }

    #[test]
    fn should_decode_window_update_increment() {
        let frame = window_update_frame(1, 1024).expect("window update");
        assert_eq!(decode_window_update(&frame).expect("increment"), 1024);
    }

    #[test]
    fn should_decode_goaway_payload() {
        let frame = goaway_frame(3, 0);
        assert_eq!(decode_goaway(&frame).expect("goaway"), (3, 0));
    }
}
