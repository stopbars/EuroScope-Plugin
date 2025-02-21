use crate::client::Client;
use crate::config::{ConfigMapping, LocalConfig};
use crate::ipc::Channel;
use crate::screen::Screen;
use crate::server::{ConnectOptions, Server};
use crate::ConnectionState;

use std::collections::VecDeque;
use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

use chrono::Utc;

use tracing::{debug, error, info, instrument, warn};

use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::ChronoUtc;
use tracing_subscriber::FmtSubscriber;

pub struct Context {
	server: Option<Server>,
	client: Option<Client>,
	messages: VecDeque<String>,
	dir: PathBuf,
	state: ConnectionState,
	tracked: Vec<String>,
}

impl Context {
	pub fn new(dir: &str) -> Option<Self> {
		std::panic::set_hook(Box::new(|info| {
			let err = Box::new(info.payload());
			if let Some(err) = err.downcast_ref::<&str>() {
				tracing::error!("panic: {err}");
			} else if let Some(err) = err.downcast_ref::<String>() {
				tracing::error!("panic: {err}");
			} else {
				tracing::error!("panic");
			}
		}));

		static LOG_PREFIX: &str = concat!(env!("CARGO_PKG_NAME"), "-");
		static LOG_SUFFIX: &str = ".log";

		fn setup_logging(dir: &Path) -> Result<()> {
			let date = Utc::now().format("%FT%T%.3fZ");
			let file_name = format!("{LOG_PREFIX}{date}{LOG_SUFFIX}");
			let file = File::create(dir.join(file_name))?;

			let subscriber = FmtSubscriber::builder()
				.with_ansi(false)
				.with_level(true)
				.with_max_level(LevelFilter::TRACE)
				.with_thread_names(true)
				.with_timer(ChronoUtc::new("%TZ".into()))
				.with_writer(file)
				.finish();

			tracing::subscriber::set_global_default(subscriber)?;

			info!("logging initialised");

			Ok(())
		}

		fn prune_logs(dir: &Path) -> Result<()> {
			let max_age = Duration::from_secs(24 * 60 * 60);

			for file in std::fs::read_dir(dir)? {
				let file = file?;

				let name = file.file_name();
				let Some(name) = name.to_str() else {
					debug!("skipped bad filename in logs dir");
					continue
				};
				if !name.starts_with(LOG_PREFIX) || !name.ends_with(LOG_SUFFIX) {
					debug!("skipped non-log file in logs dir");
					continue
				}

				let path = file.path();
				if std::fs::metadata(&path)?.modified()?.elapsed()? > max_age {
					std::fs::remove_file(&path)?;
				}
			}

			Ok(())
		}

		let logs_dir = Path::new(dir).join("log/");

		if let Err(err) = std::fs::create_dir(&logs_dir) {
			if err.kind() != ErrorKind::AlreadyExists {
				return None
			}
		}

		setup_logging(&logs_dir).ok()?;
		let _ = prune_logs(&logs_dir).inspect_err(|err| error!("log: {err}"));

		Self::try_new(dir)
			.inspect_err(|err| error!("init: {err}"))
			.ok()
	}

	#[instrument(level = "trace")]
	fn try_new(dir: &str) -> Result<Self> {
		Ok(Self {
			server: None,
			client: None,
			messages: VecDeque::new(),
			dir: dir.into(),
			state: ConnectionState::Disconnected,
			tracked: Vec::new(),
		})
	}

	#[instrument(level = "trace", skip(self))]
	pub fn tick(&mut self) {
		if let Some(server) = self.server.as_mut() {
			if server.is_cancelled() {
				debug!("disconnecting due to server cancellation");
				self.disconnect();
				self.add_message("disconnected".into());
				self.state = ConnectionState::Poisoned;
			}
		}

		if let Some(client) = self.client.as_mut() {
			if let Err(err) = client.tick() {
				warn!("{err}");
				self.disconnect();
				self.state = ConnectionState::Poisoned;
			}
		}
	}

	fn load_config(&mut self) -> Option<LocalConfig> {
		LocalConfig::load(&self.dir)
			.inspect_err(|err| {
				error!("{err}");
				self.add_message("failed to load config".into());
			})
			.ok()
	}

	fn create_server(
		&mut self,
		options: Option<ConnectOptions>,
	) -> Option<Channel> {
		let mapping = match ConfigMapping::load(&self.dir) {
			Ok(mapping) => mapping,
			Err(err) => {
				warn!("{err}");
				self.add_message("failed to load config mapping".into());
				return None
			},
		};

		match Server::new(options, mapping) {
			Ok((server, channel)) => {
				self.server = Some(server);
				Some(channel)
			},
			Err(err) => {
				warn!("(server) {err}");
				self.add_message("failed to connect".into());
				None
			},
		}
	}

	fn create_client(&mut self, channel: Channel) -> Option<()> {
		match Client::new(channel) {
			Ok(mut client) => {
				for tracked in &self.tracked {
					let _ = client.set_tracking(tracked.clone(), true);
				}

				self.client = Some(client);
				Some(())
			},
			Err(err) => {
				warn!("(client) {err}");
				self.add_message("failed to connect".into());
				self.disconnect();
				self.state = ConnectionState::Poisoned;
				None
			},
		}
	}

	#[instrument(level = "trace", skip(self))]
	pub fn connect_direct(&mut self, callsign: &str, controlling: bool) {
		if self.client.is_some() {
			warn!("connection attempted whilst connected");
			return
		}

		self.state = ConnectionState::Poisoned;

		let Some(config) = self.load_config() else {
			return
		};

		let Some(token) = config.token else {
			self.add_message("unauthenticated".into());
			return
		};

		let options = ConnectOptions {
			token,
			port: config.port,
			callsign: callsign.into(),
			controlling,
		};

		if let Some(channel) = self.create_server(Some(options)) {
			if self.create_client(channel).is_some() {
				self.state = ConnectionState::ConnectedDirect;
			}
		}
	}

	#[instrument(level = "trace", skip(self))]
	pub fn connect_proxy(&mut self) {
		if self.client.is_some() {
			warn!("connection attempted whilst connected");
			return
		}

		self.state = ConnectionState::Poisoned;

		let Some(config) = self.load_config() else {
			return
		};

		match Channel::connect(config.port) {
			Ok(channel) => {
				if self.create_client(channel).is_some() {
					self.state = ConnectionState::ConnectedProxy;
				}
			},
			Err(err) => {
				warn!("(proxy channel) {err}");
				self.add_message("failed to connect".into());
			},
		}
	}

	#[instrument(level = "trace", skip(self))]
	pub fn connect_local(&mut self) {
		if self.client.is_some() {
			warn!("connection attempted whilst connected");
			return
		}

		self.state = ConnectionState::Poisoned;

		if let Some(channel) = self.create_server(None) {
			if self.create_client(channel).is_some() {
				self.state = ConnectionState::ConnectedLocal;
			}
		}
	}

	#[instrument(level = "trace", skip(self))]
	pub fn disconnect(&mut self) {
		self.state = ConnectionState::Disconnected;

		if let Some(server) = self.server.take() {
			server.stop();
		}

		if let Some(client) = self.client.take() {
			client.disconnect();
		}
	}

	#[instrument(level = "trace", skip(self))]
	pub fn connection_state(&self) -> ConnectionState {
		self.state
	}

	#[instrument(level = "trace", skip(self))]
	pub fn next_message(&mut self) -> Option<String> {
		self.messages.pop_front()
	}

	pub fn add_message(&mut self, message: String) {
		self.messages.push_back(message)
	}

	pub fn create_screen(&mut self, geo: bool) -> Screen {
		Screen::new(self, geo)
	}

	pub fn client(&self) -> Option<&Client> {
		self.client.as_ref()
	}

	pub fn client_mut(&mut self) -> Option<&mut Client> {
		self.client.as_mut()
	}

	pub fn track_aerodrome(&mut self, icao: String) {
		if let Some(client) = self.client.as_mut() {
			if !self.tracked.contains(&icao) {
				if let Err(err) = client.set_tracking(icao.clone(), true) {
					warn!("failed to track aerodrome: {err}");
				}
			}
		}

		self.tracked.push(icao);
	}

	pub fn untrack_aerodrome(&mut self, icao: &String) {
		if let Some(i) = self.tracked.iter().position(|s| s == icao) {
			self.tracked.swap_remove(i);

			if let Some(client) = self.client.as_mut() {
				if !self.tracked.contains(icao) {
					if let Err(err) = client.set_tracking(icao.clone(), false) {
						warn!("failed to untrack aerodrome: {err}");
					}
				}
			}
		}
	}
}
