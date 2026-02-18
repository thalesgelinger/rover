/// WebSocket frame parser and builder (RFC 6455).
///
/// Zero-copy parsing: `try_parse_frame` returns offsets into the caller's buffer.
/// Server frames are never masked (RFC 6455 sec 5.1), saving 4 bytes + XOR pass.

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsOpcode {
    Continuation = 0x0,
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
}

impl WsOpcode {
    #[inline]
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0x0 => Some(Self::Continuation),
            0x1 => Some(Self::Text),
            0x2 => Some(Self::Binary),
            0x8 => Some(Self::Close),
            0x9 => Some(Self::Ping),
            0xA => Some(Self::Pong),
            _ => None,
        }
    }

    #[inline]
    pub fn is_control(self) -> bool {
        (self as u8) & 0x8 != 0
    }
}

/// Zero-copy parsed frame header -- all offsets are relative to the buffer passed to try_parse_frame.
pub struct WsFrameHeader {
    pub fin: bool,
    pub opcode: WsOpcode,
    pub masked: bool,
    pub mask: [u8; 4],
    pub payload_offset: usize,
    pub payload_len: usize,
    pub total_frame_len: usize,
}

/// Attempt to parse one complete WebSocket frame from `buf`.
///
/// Returns `None` if the buffer does not contain a complete frame yet.
/// Zero allocations -- pure offset arithmetic.
#[inline]
pub fn try_parse_frame(buf: &[u8]) -> Option<WsFrameHeader> {
    let len = buf.len();
    if len < 2 {
        return None;
    }

    let b0 = buf[0];
    let b1 = buf[1];

    let fin = b0 & 0x80 != 0;
    let opcode_val = b0 & 0x0F;
    let opcode = WsOpcode::from_u8(opcode_val)?;
    let masked = b1 & 0x80 != 0;
    let payload_len_7 = (b1 & 0x7F) as usize;

    let (payload_len, header_end) = match payload_len_7 {
        0..=125 => (payload_len_7, 2),
        126 => {
            if len < 4 {
                return None;
            }
            let pl = u16::from_be_bytes([buf[2], buf[3]]) as usize;
            (pl, 4)
        }
        // 127
        _ => {
            if len < 10 {
                return None;
            }
            let pl = u64::from_be_bytes([
                buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8], buf[9],
            ]) as usize;
            (pl, 10)
        }
    };

    let mask_size = if masked { 4 } else { 0 };
    let total_frame_len = header_end + mask_size + payload_len;

    if len < total_frame_len {
        return None;
    }

    let mut mask = [0u8; 4];
    if masked {
        mask.copy_from_slice(&buf[header_end..header_end + 4]);
    }

    let payload_offset = header_end + mask_size;

    Some(WsFrameHeader {
        fin,
        opcode,
        masked,
        mask,
        payload_offset,
        payload_len,
        total_frame_len,
    })
}

/// XOR-unmask payload in-place.
#[inline]
pub fn unmask_payload_in_place(buf: &mut [u8], mask: [u8; 4]) {
    for (i, byte) in buf.iter_mut().enumerate() {
        *byte ^= mask[i % 4];
    }
}

/// Build a server-to-client WebSocket frame into `buf`.
/// Server frames are NEVER masked (RFC 6455 sec 5.1).
#[inline]
pub fn write_frame(buf: &mut Vec<u8>, opcode: WsOpcode, payload: &[u8]) {
    let payload_len = payload.len();

    // FIN=1 | opcode
    buf.push(0x80 | (opcode as u8));

    // Length (no MASK bit for server frames)
    if payload_len <= 125 {
        buf.push(payload_len as u8);
    } else if payload_len <= 65535 {
        buf.push(126);
        buf.extend_from_slice(&(payload_len as u16).to_be_bytes());
    } else {
        buf.push(127);
        buf.extend_from_slice(&(payload_len as u64).to_be_bytes());
    }

    buf.extend_from_slice(payload);
}

/// Build a close frame with status code and optional reason.
#[inline]
pub fn write_close_frame(buf: &mut Vec<u8>, status_code: u16, reason: &str) {
    let reason_bytes = reason.as_bytes();
    let payload_len = 2 + reason_bytes.len();

    buf.push(0x88); // FIN=1 | Close
    if payload_len <= 125 {
        buf.push(payload_len as u8);
    } else {
        buf.push(126);
        buf.extend_from_slice(&(payload_len as u16).to_be_bytes());
    }

    buf.extend_from_slice(&status_code.to_be_bytes());
    buf.extend_from_slice(reason_bytes);
}

/// Build a pong frame echoing the ping's payload.
#[inline]
pub fn write_pong_frame(buf: &mut Vec<u8>, ping_payload: &[u8]) {
    write_frame(buf, WsOpcode::Pong, ping_payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_small_unmasked_frame() {
        // FIN=1, Text, payload_len=5, no mask
        let frame = [0x81, 0x05, b'h', b'e', b'l', b'l', b'o'];
        let header = try_parse_frame(&frame).unwrap();
        assert!(header.fin);
        assert_eq!(header.opcode, WsOpcode::Text);
        assert!(!header.masked);
        assert_eq!(header.payload_len, 5);
        assert_eq!(header.payload_offset, 2);
        assert_eq!(header.total_frame_len, 7);
        assert_eq!(
            &frame[header.payload_offset..header.payload_offset + header.payload_len],
            b"hello"
        );
    }

    #[test]
    fn test_parse_masked_frame() {
        // FIN=1, Text, payload_len=5, masked
        let mask = [0x37, 0xfa, 0x21, 0x3d];
        let payload = b"hello";
        let mut masked_payload = [0u8; 5];
        for (i, &b) in payload.iter().enumerate() {
            masked_payload[i] = b ^ mask[i % 4];
        }
        let mut frame = vec![0x81, 0x85]; // FIN|Text, MASK|5
        frame.extend_from_slice(&mask);
        frame.extend_from_slice(&masked_payload);

        let header = try_parse_frame(&frame).unwrap();
        assert!(header.fin);
        assert_eq!(header.opcode, WsOpcode::Text);
        assert!(header.masked);
        assert_eq!(header.mask, mask);
        assert_eq!(header.payload_len, 5);
        assert_eq!(header.payload_offset, 6);
        assert_eq!(header.total_frame_len, 11);

        // Unmask
        let mut payload_buf =
            frame[header.payload_offset..header.payload_offset + header.payload_len].to_vec();
        unmask_payload_in_place(&mut payload_buf, header.mask);
        assert_eq!(&payload_buf, b"hello");
    }

    #[test]
    fn test_parse_incomplete_returns_none() {
        assert!(try_parse_frame(&[]).is_none());
        assert!(try_parse_frame(&[0x81]).is_none());
        // Frame says 5 bytes payload but only 2 provided
        assert!(try_parse_frame(&[0x81, 0x05, b'h', b'e']).is_none());
    }

    #[test]
    fn test_write_small_frame() {
        let mut buf = Vec::new();
        write_frame(&mut buf, WsOpcode::Text, b"hello");
        assert_eq!(buf[0], 0x81); // FIN | Text
        assert_eq!(buf[1], 5); // length, no MASK bit
        assert_eq!(&buf[2..], b"hello");
    }

    #[test]
    fn test_write_medium_frame() {
        let payload = vec![0x42; 300];
        let mut buf = Vec::new();
        write_frame(&mut buf, WsOpcode::Text, &payload);
        assert_eq!(buf[0], 0x81);
        assert_eq!(buf[1], 126);
        let len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        assert_eq!(len, 300);
        assert_eq!(&buf[4..], &payload[..]);
    }

    #[test]
    fn test_write_close_frame() {
        let mut buf = Vec::new();
        write_close_frame(&mut buf, 1000, "bye");
        assert_eq!(buf[0], 0x88); // FIN | Close
        assert_eq!(buf[1], 5); // 2 (status) + 3 (reason)
        let code = u16::from_be_bytes([buf[2], buf[3]]);
        assert_eq!(code, 1000);
        assert_eq!(&buf[4..], b"bye");
    }

    #[test]
    fn test_unmask_aligned() {
        let mask = [0xAA, 0xBB, 0xCC, 0xDD];
        let mut data = vec![
            0xAA ^ b'H',
            0xBB ^ b'e',
            0xCC ^ b'l',
            0xDD ^ b'l',
            0xAA ^ b'o',
            0xBB ^ b'!',
            0xCC ^ b' ',
            0xDD ^ b' ',
        ];
        unmask_payload_in_place(&mut data, mask);
        assert_eq!(&data, b"Hello!  ");
    }

    #[test]
    fn test_unmask_odd_length() {
        let mask = [0x11, 0x22, 0x33, 0x44];
        let original = b"Hey";
        let mut masked: Vec<u8> = original
            .iter()
            .enumerate()
            .map(|(i, &b)| b ^ mask[i % 4])
            .collect();
        unmask_payload_in_place(&mut masked, mask);
        assert_eq!(&masked, original);
    }

    #[test]
    fn test_write_pong_frame() {
        let mut buf = Vec::new();
        write_pong_frame(&mut buf, b"ping!");
        assert_eq!(buf[0], 0x80 | 0x0A); // FIN | Pong
        assert_eq!(buf[1], 5);
        assert_eq!(&buf[2..], b"ping!");
    }

    #[test]
    fn test_parse_close_frame() {
        // FIN=1, Close, payload_len=5, no mask
        let mut frame = vec![0x88, 0x05];
        frame.extend_from_slice(&1000u16.to_be_bytes()); // status code
        frame.extend_from_slice(b"bye");
        let header = try_parse_frame(&frame).unwrap();
        assert!(header.fin);
        assert_eq!(header.opcode, WsOpcode::Close);
        assert_eq!(header.payload_len, 5);
    }

    #[test]
    fn test_parse_continuation_frame() {
        // FIN=0, Continuation, payload_len=3, no mask
        let frame = [0x00, 0x03, b'a', b'b', b'c'];
        let header = try_parse_frame(&frame).unwrap();
        assert!(!header.fin);
        assert_eq!(header.opcode, WsOpcode::Continuation);
        assert_eq!(header.payload_len, 3);
    }

    #[test]
    fn test_parse_binary_frame() {
        let frame = [0x82, 0x02, 0xFF, 0x00]; // FIN|Binary, 2 bytes
        let header = try_parse_frame(&frame).unwrap();
        assert!(header.fin);
        assert_eq!(header.opcode, WsOpcode::Binary);
        assert_eq!(header.payload_len, 2);
    }

    #[test]
    fn test_opcode_is_control() {
        assert!(!WsOpcode::Continuation.is_control());
        assert!(!WsOpcode::Text.is_control());
        assert!(!WsOpcode::Binary.is_control());
        assert!(WsOpcode::Close.is_control());
        assert!(WsOpcode::Ping.is_control());
        assert!(WsOpcode::Pong.is_control());
    }

    #[test]
    fn test_opcode_from_u8_invalid() {
        assert!(WsOpcode::from_u8(0x03).is_none());
        assert!(WsOpcode::from_u8(0x0B).is_none());
        assert!(WsOpcode::from_u8(0xFF).is_none());
    }

    #[test]
    fn test_write_large_frame() {
        // Test 64-bit length encoding (>65535 bytes)
        let payload = vec![0x42; 70000];
        let mut buf = Vec::new();
        write_frame(&mut buf, WsOpcode::Binary, &payload);
        assert_eq!(buf[0], 0x82); // FIN | Binary
        assert_eq!(buf[1], 127); // 64-bit length marker
        let len = u64::from_be_bytes([
            buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8], buf[9],
        ]) as usize;
        assert_eq!(len, 70000);
        assert_eq!(buf.len(), 10 + 70000);
    }

    #[test]
    fn test_roundtrip_unmask() {
        // Build a masked frame, parse it, unmask, verify payload
        let mask = [0xDE, 0xAD, 0xBE, 0xEF];
        let payload = b"WebSocket test data!";
        let mut masked_payload = Vec::with_capacity(payload.len());
        for (i, &b) in payload.iter().enumerate() {
            masked_payload.push(b ^ mask[i % 4]);
        }

        let mut frame = vec![0x81, 0x80 | (payload.len() as u8)]; // FIN|Text, MASK|len
        frame.extend_from_slice(&mask);
        frame.extend_from_slice(&masked_payload);

        let header = try_parse_frame(&frame).unwrap();
        assert!(header.masked);
        assert_eq!(header.payload_len, payload.len());

        let mut buf =
            frame[header.payload_offset..header.payload_offset + header.payload_len].to_vec();
        unmask_payload_in_place(&mut buf, header.mask);
        assert_eq!(&buf, payload);
    }

    #[test]
    fn test_unmask_unaligned_slice() {
        let mask = [0x37, 0xfa, 0x21, 0x3d];
        let payload = b"hello";
        let mut storage = vec![0u8; 16];

        for (i, &b) in payload.iter().enumerate() {
            storage[1 + i] = b ^ mask[i % 4];
        }

        unmask_payload_in_place(&mut storage[1..1 + payload.len()], mask);
        assert_eq!(&storage[1..1 + payload.len()], payload);
    }
}
