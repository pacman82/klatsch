use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::persistence::{Argument, AsArgument, FromField, GetField, GetFieldExt as _};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(Uuid);

impl UserId {
    #[cfg(test)]
    pub const fn nil() -> Self {
        Self::from_uuid(Uuid::nil())
    }

    pub const fn from_uuid(uuid: Uuid) -> Self {
        UserId(uuid)
    }

    pub fn new() -> Self {
        Self::from_uuid(Uuid::new_v4())
    }
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
        (&self.0).as_argument()
    }
}

impl FromField for UserId {
    fn from_at(row: &impl GetField, index: usize) -> Self {
        UserId::from_uuid(row.get(index))
    }
}
