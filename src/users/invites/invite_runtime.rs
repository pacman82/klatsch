use uuid::Uuid;

use super::InviteToken;

pub struct InviteRuntime {}

impl InviteRuntime {
    pub fn new() -> Self {
        InviteRuntime {}
    }

    pub fn client(&self) -> InviteClient {
        InviteClient {}
    }
}

#[cfg_attr(test, double_trait::dummies)]
pub trait Invite {
    fn new_invite(&mut self) -> anyhow::Result<InviteToken>;
    fn claim(&mut self, invitation: InviteToken) -> anyhow::Result<bool>;
}

#[derive(Clone)]
pub struct InviteClient {}

impl Invite for InviteClient {
    fn new_invite(&mut self) -> anyhow::Result<InviteToken> {
        Ok(InviteToken::from_uuid(Uuid::nil()))
    }

    fn claim(&mut self, invitation: InviteToken) -> anyhow::Result<bool> {
        Ok(true)
    }
}
