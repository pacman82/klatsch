use uuid::Uuid;

use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionId(Uuid);

impl SessionId {
    #[cfg(test)]
    pub const ALPHA: SessionId = Self::from_uuid(Uuid::from_bytes([
        0x22, 0x49, 0x05, 0x20, 0x73, 0x6d, 0x42, 0x2d, 0xa9, 0x62, 0x70, 0x70, 0x0a, 0xdd, 0x96,
        0x4c,
    ]));

    pub const fn from_uuid(uuid: Uuid) -> Self {
        SessionId(uuid)
    }

    pub fn new() -> Self {
        // We do not care about the time dimension for sessions. We do not intend to remember
        // inactive sessions anyway. So we maximize randomness and use UUID v4.
        Self::from_uuid(Uuid::new_v4())
    }
}

impl Display for SessionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for SessionId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(SessionId)
    }
}
