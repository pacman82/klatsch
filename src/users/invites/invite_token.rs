use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::persistence::{Argument, AsArgument, FromField, GetFieldNative};

/// A bearer token embedded in an invite link, granting its holder permission to create a new
/// account.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InviteToken(Uuid);

impl InviteToken {
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn new() -> Self {
        Self::from_uuid(Uuid::new_v4())
    }

    #[cfg(test)]
    pub const fn nil() -> Self {
        Self::from_uuid(Uuid::nil())
    }

    #[cfg(test)]
    pub const ALPHA: InviteToken = InviteToken::from_uuid(Uuid::from_bytes([
        0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa,
        0xaa,
    ]));
}

impl fmt::Display for InviteToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for InviteToken {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self::from_uuid)
    }
}

impl AsArgument for InviteToken {
    fn as_argument(&self) -> Argument<'_> {
        self.0.as_argument()
    }
}

impl FromField for InviteToken {
    fn from_at(row: &impl GetFieldNative, index: usize) -> Self {
        Self::from_uuid(row.get(index))
    }
}
