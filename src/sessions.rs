use uuid::Uuid;

#[cfg_attr(test, double_trait::dummies)]
pub trait Sessions {
    fn create(&mut self, user_id: Uuid) -> Uuid;
}

#[derive(Clone)]
pub struct NilSessions;

impl Sessions for NilSessions {
    fn create(&mut self, _user_id: Uuid) -> Uuid {
        Uuid::nil()
    }
}
