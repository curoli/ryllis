use std::{
    fs,
    io::ErrorKind,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub source: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub default_language: Option<String>,
    pub template: Option<PathBuf>,
    pub assets: Vec<String>,
    pub bind: Option<SocketAddr>,
}

impl Config {
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        let default_path = default_config_path();
        let Some(path) = explicit.or(default_path.as_deref()) else {
            return Ok(Self::default());
        };
        let data = match fs::read_to_string(path) {
            Err(error) if explicit.is_none() && error.kind() == ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            result => result.with_context(|| format!("reading config {}", path.display()))?,
        };
        let mut config: Self =
            toml::from_str(&data).with_context(|| format!("parsing config {}", path.display()))?;
        let directory = path.parent().unwrap_or_else(|| Path::new(""));
        config.source = config.source.map(|path| directory.join(path));
        config.output = config.output.map(|path| directory.join(path));
        config.template = config.template.map(|path| directory.join(path));
        Ok(config)
    }
}

#[cfg(unix)]
fn default_config_path() -> Option<PathBuf> {
    use std::env;

    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::home_dir().map(|home| home.join(".config")))
        .map(|directory| directory.join("ryllis/ryllis.toml"))
}

#[cfg(not(unix))]
fn default_config_path() -> Option<PathBuf> {
    Some(PathBuf::from("ryllis.toml"))
}
