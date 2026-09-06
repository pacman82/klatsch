/// Persistence operations required by the `invites` domain. Currently empty — invites are not
/// persisted yet.
#[cfg_attr(test, double_trait::dummies)]
pub trait InviteStore {}

pub struct PersistentInvite {}

impl PersistentInvite {
    pub fn new() -> Self {
        PersistentInvite {}
    }
}

impl InviteStore for PersistentInvite {}
