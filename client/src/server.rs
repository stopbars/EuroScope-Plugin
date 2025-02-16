use crate::config::ConfigMapping;
use crate::ipc::{
	BlockState, Channel, Downstream, NodeState, ServerChannel, Upstream,
};
use crate::ActivityState;

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::thread::{Builder as ThreadBuilder, JoinHandle};

use anyhow::Result;

use tokio::net::TcpListener;
use tokio::runtime::Builder as RuntimeBuilder;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot::error::TryRecvError;
use tokio::sync::{oneshot, Mutex};

use tracing::{debug, error, warn};

pub struct ConnectOptions {
	pub token: String,
	pub port: u16,
	pub callsign: String,
	pub controlling: bool,
}

pub struct Server {
	thread: JoinHandle<()>,
	shutdown: oneshot::Sender<()>,
	cancelled: oneshot::Receiver<()>,
}

impl Server {
	pub fn new(
		connect: Option<ConnectOptions>,
		mapping: ConfigMapping,
	) -> Result<(Self, Channel)> {
		let (channel, server_channel) = crate::ipc::mpsc_pair();

		let runtime = RuntimeBuilder::new_current_thread().enable_io().build()?;

		let (shutdown, srx) = tokio::sync::oneshot::channel();
		let (ctx, cancelled) = tokio::sync::oneshot::channel();

		let thread =
			ThreadBuilder::new().name("server".into()).spawn(move || {
				runtime.block_on(async {
					debug!("worker thread spawned");

					if let Err(err) = Worker::run(connect, server_channel, mapping).await
					{
						error!("{err}");
						let _ = ctx.send(());
					} else {
						let _ = srx.await;
						debug!("shutdown signal received");
					}
				})
			})?;

		Ok((
			Self {
				thread,
				shutdown,
				cancelled,
			},
			channel,
		))
	}

	pub fn is_cancelled(&mut self) -> bool {
		matches!(
			self.cancelled.try_recv(),
			Ok(()) | Err(TryRecvError::Closed)
		)
	}

	pub fn stop(self) {
		let _ = self.shutdown.send(());
		if let Err(err) = self.thread.join() {
			error!("worker thread panicked");
			if let Some(s) = err.downcast_ref::<&str>() {
				debug!("{s}");
			} else if let Some(s) = err.downcast_ref::<String>() {
				debug!("{s}");
			}
		}
	}
}

struct Aerodrome {
	config: Option<bars_config::Aerodrome>,
	state: ActivityState,
	profile: Option<String>,
	nodes: HashMap<String, NodeState>,
	blocks: HashMap<String, BlockState>,
}

#[derive(Clone)]
struct Worker {
	upstreams: UnboundedSender<Upstream>,
	clients: Arc<Mutex<Vec<UnboundedSender<Downstream>>>>,

	aerodromes: Arc<Mutex<HashMap<String, Aerodrome>>>,
	config_mapping: Arc<ConfigMapping>,
	config_cache: Arc<Mutex<Vec<Option<bars_config::Config>>>>,
}

impl Worker {
	async fn run(
		connect: Option<ConnectOptions>,
		channel: ServerChannel,
		mapping: ConfigMapping,
	) -> Result<()> {
		let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
		let config_cache = vec![None; mapping.config.len()];

		Self {
			upstreams: tx,
			clients: Arc::new(Mutex::new(Vec::new())),
			aerodromes: Arc::new(Mutex::new(HashMap::new())),
			config_mapping: Arc::new(mapping),
			config_cache: Arc::new(Mutex::new(config_cache)),
		}
		.run_impl(connect, channel, rx)
		.await
	}

	async fn run_impl(
		&self,
		connect: Option<ConnectOptions>,
		channel: ServerChannel,
		upstream_rx: UnboundedReceiver<Upstream>,
	) -> Result<()> {
		if let Some(options) = connect {
			self.bind(options.port).await?;
			self.handle_connection(upstream_rx, options).await?;
		} else {
			self.handle_local(upstream_rx).await?;
		}

		let state = self.clone();
		tokio::spawn(async move {
			if let Err(err) = state.handle_channel(channel).await {
				debug!("{err}");
			}
		});

		Ok(())
	}

	async fn bind(&self, port: u16) -> Result<()> {
		let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;

		let state = self.clone();
		tokio::spawn(async move {
			loop {
				if let Ok((stream, remote)) = listener.accept().await {
					debug!("accepted {remote}");

					let state = state.clone();
					let channel = ServerChannel::Tcp(stream);
					tokio::spawn(async move {
						if let Err(err) = state.handle_channel(channel).await {
							debug!("{err}");
						}
					});
				}
			}
		});

		Ok(())
	}

	async fn handle_connection(
		&self,
		rx: UnboundedReceiver<Upstream>,
		_options: ConnectOptions,
	) -> Result<()> {
		// TODO
		self.handle_local(rx).await
	}

	async fn handle_local(
		&self,
		mut rx: UnboundedReceiver<Upstream>,
	) -> Result<()> {
		let state = self.clone();
		tokio::spawn(async move {
			while let Some(message) = rx.recv().await {
				match message {
					Upstream::Activity {
						icao,
						state: mut activity_state,
					} => {
						debug!("updating activity for {icao}");

						let mut aerodromes = state.aerodromes.lock().await;
						let aerodrome =
							aerodromes.entry(icao.clone()).or_insert_with_key(|icao| {
								let state = state.clone();
								let icao = icao.clone();
								tokio::spawn(async move {
									if let Err(err) = state.fetch_config(&icao).await {
										warn!("{err}");
									}
								});

								Aerodrome {
									config: None,
									state: ActivityState::None,
									profile: None,
									nodes: HashMap::new(),
									blocks: HashMap::new(),
								}
							});

						if aerodrome.state == ActivityState::None
							&& activity_state == ActivityState::None
						{
							activity_state = ActivityState::Observing;
						}

						if aerodrome.state != activity_state {
							aerodrome.state = activity_state;
							state
								.broadcast(Downstream::Activity {
									icao,
									state: aerodrome.state,
								})
								.await;
						}
					},
					Upstream::Profile { icao, profile } => {
						let mut aerodromes = state.aerodromes.lock().await;
						if let Some(aerodrome) = aerodromes.get_mut(&icao) {
							state
								.broadcast(Downstream::Profile {
									icao: icao.clone(),
									profile: profile.clone(),
								})
								.await;

							aerodrome.profile = Some(profile);
						} else {
							warn!("client profile update to inactive aerodrome {icao}");
						}
					},
					Upstream::State {
						icao,
						nodes,
						blocks,
					} => {
						let mut aerodromes = state.aerodromes.lock().await;
						if let Some(aerodrome) = aerodromes.get_mut(&icao) {
							state
								.broadcast(Downstream::State {
									icao: icao.clone(),
									nodes: nodes.clone(),
									blocks: blocks.clone(),
								})
								.await;

							aerodrome.nodes.extend(nodes.into_iter());
							aerodrome.blocks.extend(blocks.into_iter());
						} else {
							warn!("client state update to inactive aerodrome {icao}");
						}
					},
					Upstream::Scenery { .. } => (),
					_ => warn!("unknown message forwarded to local handler"),
				}
			}
		});

		Ok(())
	}

	async fn handle_channel(&self, stream: ServerChannel) -> Result<()> {
		let (mut rx, mut tx) = stream.into_split();

		let (tx_tx, mut tx_rx) = tokio::sync::mpsc::unbounded_channel();
		{
			self.clients.lock().await.push(tx_tx.clone());
		}

		tokio::spawn(async move {
			while let Some(message) = tx_rx.recv().await {
				if let Err(err) = tx.send(message).await {
					debug!("{err}");
					break
				}
			}
		});

		loop {
			match rx.recv().await? {
				Upstream::Init => {
					for (icao, aerodrome) in self.aerodromes.lock().await.iter() {
						let _ = tx_tx.send(Downstream::Activity {
							icao: icao.clone(),
							state: aerodrome.state,
						});

						if let Some(config) = &aerodrome.config {
							let _ = tx_tx.send(Downstream::Config {
								data: config.clone(),
							});
						}

						if let Some(profile) = &aerodrome.profile {
							let _ = tx_tx.send(Downstream::Profile {
								icao: icao.clone(),
								profile: profile.clone(),
							});
						}

						if !aerodrome.nodes.is_empty() || !aerodrome.blocks.is_empty() {
							let _ = tx_tx.send(Downstream::State {
								icao: icao.clone(),
								nodes: aerodrome.nodes.clone(),
								blocks: aerodrome.blocks.clone(),
							});
						}
					}
				},
				message => {
					let _ = self.upstreams.send(message);
				},
			}
		}
	}

	async fn broadcast(&self, message: Downstream) {
		self
			.clients
			.lock()
			.await
			.retain(|tx| tx.send(message.clone()).is_ok());
	}

	async fn fetch_config(&self, icao: &String) -> Result<()> {
		let Some((config_i, config)) = self
			.config_mapping
			.config
			.iter()
			.enumerate()
			.find(|(_, config)| config.aerodromes.contains(icao))
		else {
			return Ok(())
		};

		let is_cached = {
			let config_cache = self.config_cache.lock().await;
			config_cache[config_i].is_some()
		};

		if !is_cached {
			debug!("fetching uncached source {:?}", config.src);

			let data = if config.src.contains("://") {
				reqwest::get(&config.src).await?.bytes().await?.to_vec()
			} else {
				let path = self.config_mapping.base.join(&config.src);
				tokio::fs::read(path).await?
			};

			let config = bars_config::Config::load(data.as_slice())?;

			let mut config_cache = self.config_cache.lock().await;
			config_cache[config_i] = Some(config);
		}

		let aerodrome = {
			let mut config_cache = self.config_cache.lock().await;
			if let Some(config) = &mut config_cache[config_i] {
				debug!("using cached config {:?} for {icao}", config.name);

				let Some(i) = config
					.aerodromes
					.iter()
					.position(|aerodrome| &aerodrome.icao == icao)
				else {
					warn!("config is missing {icao}");
					return Ok(())
				};
				config.aerodromes.swap_remove(i)
			} else {
				return Ok(())
			}
		};

		self
			.broadcast(Downstream::Config {
				data: aerodrome.clone(),
			})
			.await;

		let mut aerodromes = self.aerodromes.lock().await;
		if let Some(entry) = aerodromes.get_mut(icao) {
			entry.config = Some(aerodrome);
		}

		Ok(())
	}
}
