use std::sync::Arc;

use super::{Arguments, Persistence};

impl<T> Persistence for Arc<T>
where
    T: Persistence,
{
    type Row<'a> = T::Row<'a>;
    type Error = T::Error;
    type Connection = T::Connection;

    fn transaction<O>(
        &self,
        f: impl FnOnce(&Self::Connection) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = Result<O, anyhow::Error>> + Send
    where
        O: Send + 'static,
    {
        self.as_ref().transaction(f)
    }

    fn row<O>(
        &self,
        query: &'static str,
        args: impl Arguments + Send + Sync + 'static,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = anyhow::Result<O>> + Send
    where
        O: Send + 'static,
    {
        self.as_ref().row(query, args, map)
    }

    fn rows_vec<O>(
        &self,
        query: &'static str,
        args: impl Arguments + Send + Sync + 'static,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = anyhow::Result<Vec<O>>> + Send
    where
        O: Send + 'static,
    {
        self.as_ref().rows_vec(query, args, map)
    }
}
