use crate::Bytes;

pub struct BufferPool {
    _private: (),
}

impl BufferPool {
    pub fn new() -> Self {
        Self {
            _private: (),
        }
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for BufferPool {}
unsafe impl Sync for BufferPool {}
