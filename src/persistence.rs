mod sqlite;

use std::borrow::Cow;

use async_sqlite::rusqlite::Params;

pub use self::sqlite::SqlitePersistence;

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
        params: impl ParameterTuple + Send + Sync + 'static,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = anyhow::Result<O>> + Send
    where
        O: Send + 'static;

    fn rows_vec<O>(
        &self,
        query: &'static str,
        params: impl IntoIterator<Item = Parameter<'static>> + Send + 'static,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = anyhow::Result<Vec<O>>> + Send
    where
        O: Send + 'static;
}

pub trait FieldAccess {
    fn get_blob(&self, index: usize) -> Vec<u8>;
    fn get_i64(&self, index: usize) -> i64;
    fn get_i64_opt(&self, index: usize) -> Option<i64>;
    fn get_text(&self, index: usize) -> String;
}

pub trait ExecuteSql {
    type Row<'a>: FieldAccess;
    type Error: PersistenceError;

    fn execute(&self, query: &str, params: impl Params) -> Result<(), Self::Error>;

    fn row<O>(
        &self,
        query: &'static str,
        params: impl Params,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error>,
    ) -> Result<O, Self::Error>;
}

pub trait PersistenceError {
    fn is_unique_constraint_violation(&self) -> bool;
}

pub enum Parameter<'a> {
    I64(i64),
    Text(Cow<'a, str>),
    Blob(Cow<'a, [u8]>),
}

impl<'a> Parameter<'a> {
    pub fn borrowed(&self) -> Parameter<'_> {
        match self {
            Self::I64(value) => Parameter::I64(*value),
            Self::Text(value) => Parameter::Text(Cow::Borrowed(value.as_ref())),
            Self::Blob(value) => Parameter::Blob(Cow::Borrowed(value.as_ref())),
        }
    }
}

impl From<i64> for Parameter<'_> {
    fn from(value: i64) -> Self {
        Self::I64(value)
    }
}

pub trait ParameterTuple {
    fn get(&self, index: usize) -> Parameter<'_>;
    fn len(&self) -> usize;
}

impl ParameterTuple for () {
    fn get(&self, _index: usize) -> Parameter<'_> {
        panic!("Index out of bounds")
    }

    fn len(&self) -> usize {
        0
    }
}

impl ParameterTuple for Parameter<'_> {
    fn get(&self, index: usize) -> Parameter<'_> {
        if index == 0 {
            self.borrowed()
        } else {
            panic!("Index out of bounds")
        }
    }

    fn len(&self) -> usize {
        1
    }
}

// impl From<String> for Parameter {
//     fn from(value: String) -> Self {
//         Self::Text(value)
//     }
// }

// impl From<Vec<u8>> for Parameter {
//     fn from(value: Vec<u8>) -> Self {
//         Self::Blob(value)
//     }
// }
