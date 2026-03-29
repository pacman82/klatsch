mod parameters;
mod sqlite;

pub use self::{
    parameters::{Parameter, Parameters},
    sqlite::SqlitePersistence,
};

#[cfg_attr(test, double_trait::dummies)]
pub trait Persistence {
    type Row<'a>: FieldAccess;
    type Error: PersistenceError;
    type Connection: ExecuteSql<Error = Self::Error>;

    fn transaction<O>(
        &self,
        f: impl FnOnce(&Self::Connection) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = Result<O, anyhow::Error>> + Send
    where
        O: Send + 'static;

    fn row<O>(
        &self,
        query: &'static str,
        params: impl Parameters + Send + Sync + 'static,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = anyhow::Result<O>> + Send
    where
        O: Send + 'static;

    fn rows_vec<O>(
        &self,
        query: &'static str,
        params: impl Parameters + Send + Sync + 'static,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = anyhow::Result<Vec<O>>> + Send
    where
        O: Send + 'static;
}

#[cfg_attr(test, double_trait::dummies)]
pub trait FieldAccess {
    fn get_blob(&self, index: usize) -> Vec<u8>;
    fn get_i64(&self, index: usize) -> i64;
    fn get_i64_opt(&self, index: usize) -> Option<i64>;
    fn get_text(&self, index: usize) -> String;
}

#[cfg_attr(test, double_trait::dummies)]
pub trait ExecuteSql {
    type Row<'a>: FieldAccess;
    type Error: PersistenceError;

    fn execute(&self, query: &str, params: impl Parameters) -> Result<(), Self::Error>;

    fn row<O>(
        &self,
        query: &'static str,
        params: impl Parameters,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error>,
    ) -> Result<O, Self::Error>;
}

#[cfg_attr(test, double_trait::dummies)]
pub trait PersistenceError {
    fn is_unique_constraint_violation(&self) -> bool;
}
