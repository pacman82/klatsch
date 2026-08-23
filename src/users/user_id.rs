use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::persistence::{Argument, AsArgument, FromField, GetFieldNative};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(Uuid);

impl UserId {
    const fn from_uuid(uuid: Uuid) -> Self {
        UserId(uuid)
    }

    pub fn new() -> Self {
        Self::from_uuid(Uuid::new_v4())
    }

    #[cfg(test)]
    pub const fn nil() -> Self {
        Self::from_uuid(Uuid::nil())
    }

    #[cfg(test)]
    pub const ALICE: UserId = UserId::from_uuid(Uuid::from_bytes([
        0xab, 0x70, 0xb6, 0xca, 0x41, 0x39, 0x49, 0x9f, 0xa6, 0x6d, 0x15, 0xe8, 0x8f, 0x08, 0x1f,
        0xb1,
    ]));

    #[cfg(test)]
    pub const BOB: UserId = UserId::from_uuid(Uuid::from_bytes([
        0x01, 0x96, 0x52, 0x3e, 0xf3, 0x61, 0x7c, 0x62, 0xb4, 0x88, 0xad, 0x5a, 0x9a, 0x30, 0x02,
        0x1c,
    ]));
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for UserId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(UserId)
    }
}

impl AsArgument for UserId {
    fn as_argument(&self) -> Argument<'_> {
        self.0.as_argument()
    }
}

impl AsArgument for &UserId {
    fn as_argument(&self) -> Argument<'_> {
        self.0.as_argument()
    }
}

impl FromField for UserId {
    fn from_at(row: &impl GetFieldNative, index: usize) -> Self {
        UserId::from_uuid(row.get(index))
    }
}
