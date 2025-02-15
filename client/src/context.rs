use crate::ConnectionState;

use std::collections::VecDeque;
use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use chrono::Utc;

use tracing::{debug, error, info, instrument};

use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::ChronoUtc;
use tracing_subscriber::FmtSubscriber;

pub struct Context {
	messages: VecDeque<String>,
}

impl Context {
	pub fn new(dir: &str) -> Option<Self> {
		static LOG_PREFIX: &str = concat!(env!("CARGO_PKG_NAME"), "-");
		static LOG_SUFFIX: &str = ".log";

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
		let mut this = Self {
			messages: VecDeque::new(),
		};

		this.add_message("hello!".into());

		Ok(this)
	}

	#[instrument(level = "trace", skip(self))]
	pub fn tick(&mut self) {
		//
	}

	#[instrument(level = "trace", skip(self))]
	pub fn connect_direct(&mut self, callsign: &str, controlling: bool) {
		todo!()
	}

	#[instrument(level = "trace", skip(self))]
	pub fn connect_proxy(&mut self) {
		todo!()
	}

	#[instrument(level = "trace", skip(self))]
	pub fn connect_local(&mut self) {
		todo!()
	}

	#[instrument(level = "trace", skip(self))]
	pub fn disconnect(&mut self) {
		todo!()
	}

	#[instrument(level = "trace", skip(self))]
	pub fn connection_state(&self) -> ConnectionState {
		todo!()
	}

	#[instrument(level = "trace", skip(self))]
	pub fn next_message(&mut self) -> Option<String> {
		self.messages.pop_front()
	}

	pub fn add_message(&mut self, message: String) {
		self.messages.push_back(message)
	}
}
