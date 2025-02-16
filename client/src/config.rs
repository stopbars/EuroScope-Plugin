use std::path::{Path, PathBuf};

use anyhow::Result;

use serde::{Deserialize, Serialize};

const DEFAULT_PORT: u16 = 6866;

fn default_port() -> u16 {
	DEFAULT_PORT
}

#[derive(Default, Deserialize, Serialize)]
pub struct LocalConfig {
	pub token: Option<String>,
	#[serde(default = "default_port")]
	pub port: u16,
}

impl LocalConfig {
	pub fn load(dir: &Path) -> Result<Self> {
		let p = dir.join("local.toml");
		if std::fs::exists(&p)? {
			let s = std::fs::read_to_string(&p)?;
			Ok(toml::from_str(&s)?)
		} else {
			Ok(Self::default())
		}
	}
}

#[derive(Default, Deserialize, Serialize)]
pub struct ConfigMapping {
	pub config: Vec<ConfigSource>,
	#[serde(default)]
	pub base: PathBuf,
}

impl ConfigMapping {
	pub fn load(dir: &Path) -> Result<Self> {
		let p = dir.join("config.toml");
		if std::fs::exists(&p)? {
			let s = std::fs::read_to_string(&p)?;
			Ok(Self {
				base: dir.into(),
				..toml::from_str(&s)?
			})
		} else {
			Ok(Self::default())
		}
	}
}

#[derive(Deserialize, Serialize)]
pub struct ConfigSource {
	pub src: String,
	pub aerodromes: Vec<String>,
}
