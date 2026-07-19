use std::{
    env::{self, VarError},
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use anyhow::{Context, anyhow};

use crate::sessions::SessionExpiry;

/// Session idle timeout if SESSION_IDLE_TIMEOUT is not set.
const DEFAULT_SESSION_IDLE_TIMEOUT: Duration = Duration::from_hours(30 * 24);

/// Session lifetime cap if SESSION_MAX_LIFETIME is not set.
const DEFAULT_SESSION_MAX_LIFETIME: Duration = Duration::from_hours(90 * 24);

/// All static configuration for the application. I.e. configuration which does not change during
/// the runtime without a restart.
pub struct Configuration {
    /// The port we bind to.
    port: u16,
    /// Host name or IP address to bind to.
    host: String,
    /// Directory for persistent storage. If not set, the database is in-memory only.
    persistence_dir: Option<PathBuf>,
    /// When sessions expire.
    session_expiry: SessionExpiry,
}

impl Configuration {
    /// Load the configuration from the environment variables.
    pub fn from_env() -> anyhow::Result<Self> {
        let host = extract_env_var("HOST")?.unwrap_or_else(|| "0.0.0.0".to_owned());
        let port = extract_env_var("PORT")?.unwrap_or(3000);
        let persistence = extract_bool_env_var("PERSISTENCE")?.unwrap_or(true);
        let persistence_dir = if persistence {
            Some(extract_env_var("PERSISTENCE_DIRECTORY")?.unwrap_or_else(|| "data".into()))
        } else {
            None
        };

        let session_expiry = SessionExpiry {
            idle_timeout: extract_duration_env_var("SESSION_IDLE_TIMEOUT")?
                .unwrap_or(DEFAULT_SESSION_IDLE_TIMEOUT),
            max_lifetime: extract_duration_env_var("SESSION_MAX_LIFETIME")?
                .unwrap_or(DEFAULT_SESSION_MAX_LIFETIME),
        };

        let cfg = Configuration {
            host,
            port,
            persistence_dir,
            session_expiry,
        };
        Ok(cfg)
    }

    /// The address the server should bind to.
    pub fn socket_addr(&self) -> (&str, u16) {
        (&self.host, self.port)
    }

    /// Directory for persistent storage, if configured.
    pub fn persistence_dir(&self) -> Option<&Path> {
        self.persistence_dir.as_deref()
    }

    /// When sessions expire.
    pub fn session_expiry(&self) -> SessionExpiry {
        self.session_expiry
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

fn extract_duration_env_var(var_name: &str) -> anyhow::Result<Option<Duration>> {
    parse_duration_from_env_result(var_name, env::var(var_name))
}

fn parse_duration_from_env_result(
    var_name: &str,
    result: Result<String, VarError>,
) -> anyhow::Result<Option<Duration>> {
    handle_invalid_unicode(result)?
        .map(|s| {
            humantime::parse_duration(&s).map_err(|_| {
                anyhow!("{var_name} must be a duration like '30d', '12h' or '90m', got '{s}'")
            })
        })
        .transpose()
}

fn extract_bool_env_var(var_name: &str) -> anyhow::Result<Option<bool>> {
    let value = handle_invalid_unicode(env::var(var_name))?;
    match value.as_deref() {
        None => Ok(None),
        Some(s) if s.eq_ignore_ascii_case("true") => Ok(Some(true)),
        Some(s) if s.eq_ignore_ascii_case("false") => Ok(Some(false)),
        Some(s) => Err(anyhow!(
            "{var_name} must be 'true' or 'false' (case insensitive), got '{s}'"
        )),
    }
}

fn extract_env_var<T>(var_name: &str) -> anyhow::Result<Option<T>>
where
    T: FromStr,
    T::Err: Into<anyhow::Error> + Send + Sync + std::error::Error + 'static,
{
    parse_from_env_result(env::var(var_name))
        .with_context(|| format!("Invalid environment variable '{var_name}'"))
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, time::Duration};

    use super::*;

    #[test]
    fn session_timeouts_are_human_readable_durations() {
        let result =
            parse_duration_from_env_result("SESSION_IDLE_TIMEOUT", Ok("30days".to_owned()));

        let duration = result.unwrap().expect("value is present");

        assert_eq!(duration, Duration::from_hours(30 * 24));
    }

    #[test]
    fn invalid_duration_error_names_variable_and_expected_format() {
        let result =
            parse_duration_from_env_result("SESSION_IDLE_TIMEOUT", Ok("banana".to_owned()));

        let error = result.unwrap_err();

        assert_eq!(
            "SESSION_IDLE_TIMEOUT must be a duration like '30d', '12h' or '90m', got 'banana'",
            error.to_string()
        );
        assert!(error.source().is_none(), "message must stand alone");
    }

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
