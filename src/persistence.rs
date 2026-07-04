mod arguments;
mod migrate;
mod shared;
mod sqlite;

use uuid::Uuid;

pub use self::{
    arguments::{Argument, Arguments, AsArgument},
    migrate::migrate,
    sqlite::SqlitePersistence,
};

pub trait Persistence {
    type Row<'a>: GetField;
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
        args: impl Arguments + Send + Sync + 'static,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = anyhow::Result<O>> + Send
    where
        O: Send + 'static;

    fn rows_vec<O>(
        &self,
        query: &'static str,
        args: impl Arguments + Send + Sync + 'static,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = anyhow::Result<Vec<O>>> + Send
    where
        O: Send + 'static;
}

pub trait GetField {
    fn get_uuid(&self, index: usize) -> Uuid;
    fn get_i64(&self, index: usize) -> i64;
    fn get_i64_opt(&self, index: usize) -> Option<i64>;
    fn get_text(&self, index: usize) -> String;
    fn get_text_opt(&self, index: usize) -> Option<String>;
}

pub trait FromField {
    fn from_at(row: &impl GetField, index: usize) -> Self;
}

impl FromField for Uuid {
    fn from_at(row: &impl GetField, index: usize) -> Self {
        row.get_uuid(index)
    }
}

impl FromField for i64 {
    fn from_at(row: &impl GetField, index: usize) -> Self {
        row.get_i64(index)
    }
}

pub trait GetFieldExt<T> {
    fn get(&self, index: usize) -> T;
}

impl<T, R> GetFieldExt<T> for R
where
    T: FromField,
    R: GetField,
{
    fn get(&self, index: usize) -> T {
        T::from_at(self, index)
    }
}

pub trait ExecuteSql {
    type Row<'a>: GetField;
    type Error: PersistenceError;

    fn execute(&self, query: &str, args: impl Arguments) -> Result<(), Self::Error>;

    fn row<O>(
        &self,
        query: &'static str,
        args: impl Arguments,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error>,
    ) -> Result<O, Self::Error>;

    fn rows_vec<O>(
        &self,
        query: &str,
        args: impl Arguments,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error>,
    ) -> Result<Vec<O>, Self::Error>;
}

#[cfg_attr(test, double_trait::dummies)]
pub trait PersistenceError {
    fn is_unique_constraint_violation(&self) -> bool;
}
