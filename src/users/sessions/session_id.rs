use uuid::Uuid;

use std::{
    borrow::Cow,
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use crate::persistence::{Argument, AsArgument, FromField, GetFieldNative};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionId(Uuid);

impl SessionId {
    #[cfg(test)]
    pub const ALICE: SessionId = Self::from_uuid(Uuid::from_bytes([
        0x22, 0x49, 0x05, 0x20, 0x73, 0x6d, 0x42, 0x2d, 0xa9, 0x62, 0x70, 0x70, 0x0a, 0xdd, 0x96,
        0x4c,
    ]));

    #[cfg(test)]
    pub const BOB: SessionId = Self::from_uuid(Uuid::from_bytes([
        0x81, 0x3f, 0xe4, 0x6a, 0x1c, 0x0b, 0x4e, 0x52, 0xb7, 0x09, 0x24, 0x5d, 0x6f, 0x83, 0x1a,
        0xd7,
    ]));

    #[cfg(test)]
    pub const fn nil() -> Self {
        Self::from_uuid(Uuid::nil())
    }

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

impl AsArgument for SessionId {
    fn as_argument(&self) -> Argument<'_> {
        self.0.as_argument()
    }
}

impl FromField for SessionId {
    fn from_at(row: &impl GetFieldNative, index: usize) -> Self {
        SessionId::from_uuid(row.get(index))
    }
}

/// A one-way hash of a [`SessionId`]. Session ids act as bearer tokens, so neither the in-memory
/// store nor persistence retain the plain id — only this hash, which is what they key sessions by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionHash([u8; 32]);

impl SessionHash {
    pub fn from_session_id(session_id: SessionId) -> Self {
        SessionHash(*blake3::hash(session_id.0.as_bytes()).as_bytes())
    }
}

impl AsArgument for SessionHash {
    fn as_argument(&self) -> Argument<'_> {
        Argument::Bytes(Cow::Borrowed(&self.0))
    }
}

impl FromField for SessionHash {
    fn from_at(row: &impl GetFieldNative, index: usize) -> Self {
        let bytes: Vec<u8> = row.get(index);
        let bytes: [u8; 32] = bytes
            .try_into()
            .expect("session hash column must be 32 bytes");
        SessionHash(bytes)
    }
}
