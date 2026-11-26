use std::env;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub limit: Option<usize>,
}

impl Config {
    fn validate(&self) -> Result<(), String> {
        if self.limit == Some(0) {
            return Err(format!(
                "limit in {} must be at least 1",
                config_path().display()
            ));
        }
        Ok(())
    }
}

pub fn load() -> Result<Config, String> {
    let path = config_path();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Config::default());
        }
        Err(error) => return Err(format!("could not read {}: {error}", path.display())),
    };
    let config: Config = toml::from_str(&text)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))?;
    config.validate()?;
    Ok(config)
}

pub fn config_path() -> PathBuf {
    config_home()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dictionary")
        .join("config.toml")
}

fn config_home() -> Option<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    #[cfg(windows)]
    if let Some(path) = env::var_os("APPDATA")
        && !path.is_empty()
    {
        return Some(PathBuf::from(path));
    }
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".config"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_valid_limit() {
        let config: Config = toml::from_str("limit = 2").unwrap();
        config.validate().unwrap();
        assert_eq!(config.limit, Some(2));
    }

    #[test]
    fn rejects_zero_and_unknown_settings() {
        let invalid: Config = toml::from_str("limit = 0").unwrap();
        assert!(invalid.validate().is_err());
        assert!(toml::from_str::<Config>("limt = 2").is_err());
    }

    #[test]
    fn locates_the_standard_config_path() {
        assert!(config_path().ends_with("dictionary/config.toml"));
    }
}
