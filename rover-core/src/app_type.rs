#[repr(i64)]
pub enum AppType {
    Server,
}

impl AppType {
    pub fn to_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(value: i64) -> Option<Self> {
        match value {
            0 => Some(AppType::Server),
            _ => None,
        }
    }
}
