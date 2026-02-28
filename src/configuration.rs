use std::{
    env::{self, VarError},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::Context;

/// All static configuration for the application. I.e. configuration which does not change during
/// the runtime without a restart.
pub struct Configuration {
    /// The port we bind to.
    port: u16,
    /// Host name or IP address to bind to.
    host: String,
    /// Path to the SQLite database file. If not set, the database is in-memory only.
    database_path: Option<PathBuf>,
}

impl Configuration {
    /// Load the configuration from the environment variables.
    pub fn from_env() -> anyhow::Result<Self> {
        let host = extract_env_var("HOST")?.unwrap_or_else(|| "0.0.0.0".to_owned());
        let port = extract_env_var("PORT")?.unwrap_or(3000);
        let database_path = extract_env_var("DATABASE_PATH")?;

        let cfg = Configuration {
            host,
            port,
            database_path,
        };
        Ok(cfg)
    }

    /// The address the server should bind to.
    pub fn socket_addr(&self) -> (&str, u16) {
        (&self.host, self.port)
    }

    /// Path to the SQLite database file, if configured.
    pub fn database_path(&self) -> Option<&Path> {
        self.database_path.as_deref()
    }
}

fn handle_invalid_unicode(result: Result<String, VarError>) -> anyhow::Result<Option<String>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(VarError::NotPresent) => Ok(None),
        Err(e @ VarError::NotUnicode(_)) => Err(e.into()),
    }
}

fn parse_from_env_result<T>(result: Result<String, VarError>) -> anyhow::Result<Option<T>>
where
    T: FromStr,
    T::Err: Into<anyhow::Error> + Send + Sync + std::error::Error + 'static,
{
    let value = handle_invalid_unicode(result)?
        .map(|value| value.parse::<T>())
        .transpose()?;
    Ok(value)
}

fn extract_env_var<T>(var_name: &str) -> anyhow::Result<Option<T>>
where
    T: FromStr,
    T::Err: Into<anyhow::Error> + Send + Sync + std::error::Error + 'static,
{
    parse_from_env_result(env::var(var_name))
        .with_context(|| format!("Error parsing environment variable '{var_name}'"))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    #[test]
    fn test_handle_invalid_unicode() {
        let result = Err(VarError::NotUnicode(OsString::from("Hello")));

        let result = handle_invalid_unicode(result);

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(
            "environment variable was not valid unicode: \"Hello\"",
            error.to_string()
        );
    }
}
