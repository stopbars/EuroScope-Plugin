use bars_config::{Aerodrome, Config};

use std::path::{Path, PathBuf};

use anyhow::Result;

use serde::{Deserialize, Serialize};

use tracing::{debug, warn};

const DEFAULT_PORT: u16 = 6866;

fn default_port() -> u16 {
	DEFAULT_PORT
}

fn default_server() -> String {
	"https://v2.stopbars.com/".into()
}

#[derive(Default, Deserialize, Serialize)]
pub struct LocalConfig {
	pub token: Option<String>,
	#[serde(default = "default_port")]
	pub port: u16,
	#[serde(default = "default_server")]
	pub server: String,
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

pub struct ConfigManager {
	sources: Vec<(ConfigSource, Option<Config>)>,
	base: PathBuf,
}

impl ConfigManager {
	pub fn new(mapping: ConfigMapping) -> Self {
		Self {
			sources: mapping
				.config
				.into_iter()
				.map(|source| (source, None))
				.collect(),
			base: mapping.base,
		}
	}

	pub async fn load(&mut self, icao: &String) -> Result<Option<Aerodrome>> {
		let Some((source, config)) = self
			.sources
			.iter_mut()
			.find(|(source, _)| source.aerodromes.contains(icao))
		else {
			warn!("requested aerodrome {icao} has no mapped config source");
			return Ok(None)
		};

		if config.is_none() {
			debug!("fetching uncached source {:?}", source.src);

			let data = if source.src.contains("://") {
				reqwest::get(&source.src).await?.bytes().await?.to_vec()
			} else {
				let path = self.base.join(&source.src);
				tokio::fs::read(path).await?
			};

			*config = Some(Config::load(data.as_slice())?);
		}

		let config = config.as_mut().unwrap();

		let Some(i) = config
			.aerodromes
			.iter()
			.position(|aerodrome| &aerodrome.icao == icao)
		else {
			warn!("loaded config source is missing advertised {icao}");
			return Ok(None)
		};

		debug!("loaded {icao} from {:?}", source.src);

		Ok(Some(config.aerodromes.swap_remove(i)))
	}
}
