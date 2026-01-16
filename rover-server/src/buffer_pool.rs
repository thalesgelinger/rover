use crate::Bytes;

// Increased pool sizes for high-load scenarios (2000+ connections)
const POOL_SIZE_SMALL: usize = 256;
const POOL_SIZE_MEDIUM: usize = 128;
const POOL_SIZE_LARGE: usize = 64;
const RESPONSE_POOL_SIZE: usize = 512;
const JSON_POOL_SIZE: usize = 512;
const OFFSET_POOL_SIZE: usize = 256;

pub struct BufferPool {
    /// Pool for query/header pairs with capacity 8
    bytes_pairs_8: Vec<Vec<(Bytes, Bytes)>>,
    /// Pool for query/header pairs with capacity 16
    bytes_pairs_16: Vec<Vec<(Bytes, Bytes)>>,
    /// Pool for query/header pairs with capacity 32
    bytes_pairs_32: Vec<Vec<(Bytes, Bytes)>>,
    /// Pool for response buffers (512 bytes typical)
    response_bufs: Vec<Vec<u8>>,
    /// Pool for JSON serialization buffers (256 bytes typical)
    json_bufs: Vec<Vec<u8>>,
    /// Pool for query/header offset vectors (tuple of offsets)
    offset_vecs: Vec<Vec<(u16, u8, u16, u16)>>,
}

impl BufferPool {
    pub fn new() -> Self {
        let mut bytes_pairs_8 = Vec::with_capacity(POOL_SIZE_SMALL);
        let mut bytes_pairs_16 = Vec::with_capacity(POOL_SIZE_MEDIUM);
        let mut bytes_pairs_32 = Vec::with_capacity(POOL_SIZE_LARGE);
        let mut response_bufs = Vec::with_capacity(RESPONSE_POOL_SIZE);
        let mut json_bufs = Vec::with_capacity(JSON_POOL_SIZE);
        let mut offset_vecs = Vec::with_capacity(OFFSET_POOL_SIZE);

        for _ in 0..POOL_SIZE_SMALL {
            bytes_pairs_8.push(Vec::with_capacity(8));
        }
        for _ in 0..POOL_SIZE_MEDIUM {
            bytes_pairs_16.push(Vec::with_capacity(16));
        }
        for _ in 0..POOL_SIZE_LARGE {
            bytes_pairs_32.push(Vec::with_capacity(32));
        }
        for _ in 0..RESPONSE_POOL_SIZE {
            response_bufs.push(Vec::with_capacity(512));
        }
        for _ in 0..JSON_POOL_SIZE {
            json_bufs.push(Vec::with_capacity(256));
        }
        for _ in 0..OFFSET_POOL_SIZE {
            offset_vecs.push(Vec::with_capacity(16));
        }

        Self {
            bytes_pairs_8,
            bytes_pairs_16,
            bytes_pairs_32,
            response_bufs,
            json_bufs,
            offset_vecs,
        }
    }

    /// Get a pooled bytes pair buffer based on expected size
    #[inline]
    pub fn get_bytes_pairs(&mut self, expected_size: usize) -> Vec<(Bytes, Bytes)> {
        if expected_size <= 8 {
            self.bytes_pairs_8
                .pop()
                .unwrap_or_else(|| Vec::with_capacity(8))
        } else if expected_size <= 16 {
            self.bytes_pairs_16
                .pop()
                .unwrap_or_else(|| Vec::with_capacity(16))
        } else {
            self.bytes_pairs_32
                .pop()
                .unwrap_or_else(|| Vec::with_capacity(32))
        }
    }

    /// Return a bytes pair buffer to the pool
    #[inline]
    pub fn return_bytes_pairs(&mut self, mut buf: Vec<(Bytes, Bytes)>) {
        buf.clear();
        let cap = buf.capacity();
        if cap <= 8 && self.bytes_pairs_8.len() < POOL_SIZE_SMALL {
            self.bytes_pairs_8.push(buf);
        } else if cap <= 16 && self.bytes_pairs_16.len() < POOL_SIZE_MEDIUM {
            self.bytes_pairs_16.push(buf);
        } else if cap <= 32 && self.bytes_pairs_32.len() < POOL_SIZE_LARGE {
            self.bytes_pairs_32.push(buf);
        }
        // Drop if pool is full
    }

    /// Get a pooled response buffer
    #[inline]
    pub fn get_response_buf(&mut self) -> Vec<u8> {
        self.response_bufs
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(512))
    }

    /// Return a response buffer to the pool
    #[inline]
    pub fn return_response_buf(&mut self, mut buf: Vec<u8>) {
        buf.clear();
        if self.response_bufs.len() < RESPONSE_POOL_SIZE {
            self.response_bufs.push(buf);
        }
        // Drop if pool is full
    }

    /// Get a pooled JSON buffer
    #[inline]
    pub fn get_json_buf(&mut self) -> Vec<u8> {
        self.json_bufs
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(256))
    }

    /// Return a JSON buffer to the pool
    #[inline]
    pub fn return_json_buf(&mut self, mut buf: Vec<u8>) {
        buf.clear();
        if self.json_bufs.len() < JSON_POOL_SIZE {
            self.json_bufs.push(buf);
        }
        // Drop if pool is full
    }

    /// Get a pooled offset vector for query/header parsing
    #[inline]
    pub fn get_offset_vec(&mut self) -> Vec<(u16, u8, u16, u16)> {
        self.offset_vecs
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(16))
    }

    /// Return an offset vector to the pool
    #[inline]
    pub fn return_offset_vec(&mut self, mut vec: Vec<(u16, u8, u16, u16)>) {
        vec.clear();
        if self.offset_vecs.len() < OFFSET_POOL_SIZE {
            self.offset_vecs.push(vec);
        }
        // Drop if pool is full
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for BufferPool {}
unsafe impl Sync for BufferPool {}
