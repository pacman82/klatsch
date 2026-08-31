pub struct InviteRuntime {}

impl InviteRuntime {
    pub fn new() -> Self {
        InviteRuntime {}
    }

    pub fn client(&self) -> InviteClient {
        InviteClient {}
    }
}

#[derive(Clone)]
pub struct InviteClient {}
