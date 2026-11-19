use std::{
    fs,
    io::ErrorKind,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub source: PathBuf,
    pub output: PathBuf,
    pub default_language: String,
    pub template: Option<PathBuf>,
    pub bind: SocketAddr,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source: "content".into(),
            output: "target/ryllis-site".into(),
            default_language: "en".into(),
            template: None,
            bind: SocketAddr::from(([127, 0, 0, 1], 3000)),
        }
    }
}

impl Config {
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        let path = explicit.unwrap_or_else(|| Path::new("ryllis.toml"));
        let data = match fs::read_to_string(path) {
            Err(error) if explicit.is_none() && error.kind() == ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            result => result.with_context(|| format!("reading config {}", path.display()))?,
        };
        let mut config: Self =
            toml::from_str(&data).with_context(|| format!("parsing config {}", path.display()))?;
        let directory = path.parent().unwrap_or_else(|| Path::new(""));
        config.source = directory.join(config.source);
        config.output = directory.join(config.output);
        config.template = config.template.map(|path| directory.join(path));
        Ok(config)
    }
}
