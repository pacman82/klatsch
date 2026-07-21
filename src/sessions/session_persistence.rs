#[cfg_attr(test, double_trait::dummies)]
pub trait SessionPersistence {}

/// A [`SessionPersistence`] which does not persist anything. Sessions do not survive a restart.
pub struct NoPersistence;

impl SessionPersistence for NoPersistence {}
