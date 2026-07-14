mod arguments;
mod migrate;
mod sqlite;

use uuid::Uuid;

pub use self::{
    arguments::{Argument, Arguments, AsArgument},
    migrate::migrate,
    sqlite::SqlitePersistence,
};

pub trait ExecuteSqlAsync {
    type Row<'a>: GetFieldNative;
    type Error: PersistenceError;
    type Connection: ExecuteSqlSync<Error = Self::Error>;

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

/// Rows allow access to types natively supported by persistence
pub trait GetFieldNative:
    GetField<i64> + GetField<Uuid> + GetField<Option<i64>> + GetField<String> + GetField<Option<String>>
{
}

/// A trait intended to be implemented by types more native to the domain than persistence. There is
/// a blanket implementation implementing [`GetField`] for any `T` implementing [`FromField`]
pub trait FromField {
    fn from_at(row: &impl GetFieldNative, index: usize) -> Self;
}

/// Access fields of type `T` of a row.
pub trait GetField<T> {
    fn get(&self, index: usize) -> T;
}

impl<T, R> GetField<T> for R
where
    T: FromField,
    R: GetFieldNative,
{
    fn get(&self, index: usize) -> T {
        T::from_at(self, index)
    }
}

pub trait ExecuteSqlSync {
    type Row<'a>: GetFieldNative;
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
