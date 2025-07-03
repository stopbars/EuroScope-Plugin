use crate::config::{ConfigManager, ConfigMapping};
use crate::ipc::{Channel, Downstream, ServerChannel, Upstream};

use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::thread::{Builder as ThreadBuilder, JoinHandle};
use std::time::{Duration, Instant};

use bars_config::Aerodrome;
use bars_protocol::{
	Downstream as NetDownstream, Patch, State, Upstream as NetUpstream,
};

use anyhow::Result;

use futures::sink::SinkExt;
use futures::stream::StreamExt;

use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Builder as RuntimeBuilder;
use tokio::sync::broadcast::Sender;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot::error::TryRecvError;
use tokio::sync::{mpsc, oneshot, Mutex};

use tokio_tungstenite::tungstenite::http::Uri;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use tracing::{debug, error, trace, warn};

const SOCKET_POLL_TIMEOUT: Duration = Duration::from_millis(100);
const STATE_POLL_INTERVAL: Duration = Duration::from_secs(30);

pub struct ConnectOptions {
	pub server: String,
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

		let runtime = RuntimeBuilder::new_current_thread()
			.enable_io()
			.enable_time()
			.build()?;

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
			if let Some(s) = err
				.downcast_ref::<&str>()
				.or(err.downcast_ref::<String>().map(|s| s.as_str()).as_ref())
			{
				debug!("{s}");
			}
		}
	}
}

#[derive(Clone)]
struct Worker {
	broadcast: Sender<Downstream>,
}

impl Worker {
	async fn run(
		connect: Option<ConnectOptions>,
		channel: ServerChannel,
		mapping: ConfigMapping,
	) -> Result<()> {
		let (tx, rx) = mpsc::unbounded_channel();

		let this = Self {
			broadcast: Sender::new(16),
		};

		this.handle_stream(channel, tx.clone()).await?;

		if let Some(options) = &connect {
			this.bind(options.port, tx).await?;
		}

		tokio::spawn(async move {
			let _ = this.serve(connect, mapping, rx).await;
		});

		Ok(())
	}

	async fn serve(
		&self,
		connect: Option<ConnectOptions>,
		mapping: ConfigMapping,
		mut rx: UnboundedReceiver<Upstream>,
	) -> Result<()> {
		let mut aerodromes = HashMap::new();
		let config = Arc::new(Mutex::new(ConfigManager::new(mapping)));

		while let Some(message) = rx.recv().await {
			let Some(icao) = message.icao() else {
				warn!("unknown message forwarded to local handler");
				break
			};

			if !aerodromes.contains_key(icao) {
				let aerodrome = AerodromeManager::new(
					icao,
					&connect,
					config.clone(),
					self.broadcast.clone(),
				)
				.await?;
				aerodromes.insert(icao.clone(), aerodrome);
			}

			let aerodrome = aerodromes.get_mut(icao).unwrap();

			let res = match message {
				Upstream::Track { icao, track } => {
					debug!("updating tracking for {icao} ({track})");
					aerodrome.track(track).await
				},
				Upstream::Control { icao, control } => {
					debug!("updating controlling for {icao} ({control})");
					aerodrome.control(control).await;
					Ok(())
				},
				Upstream::Patch { icao, patch } => {
					debug!("patching {icao}");
					aerodrome.patch(patch).await
				},
				Upstream::Scenery { icao, scenery } => {
					debug!("updating {icao}");
					aerodrome.scenery(scenery).await
				},
				_ => Ok(()),
			};

			trace!("end message processing");

			if let Err(err) = res {
				warn!("{err}");
			}
		}

		Ok(())
	}

	async fn bind(
		&self,
		port: u16,
		server_tx: UnboundedSender<Upstream>,
	) -> Result<()> {
		let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;

		let state = self.clone();
		tokio::spawn(async move {
			loop {
				if let Ok((stream, remote)) = listener.accept().await {
					debug!("accepted {remote}");

					let channel = ServerChannel::Tcp(stream);
					if let Err(err) =
						state.handle_stream(channel, server_tx.clone()).await
					{
						debug!("{err}");
					}
				}
			}
		});

		Ok(())
	}

	async fn handle_stream(
		&self,
		stream: ServerChannel,
		server_tx: UnboundedSender<Upstream>,
	) -> Result<()> {
		let (mut stream_rx, mut stream_tx) = stream.into_split();
		let mut ipc_rx = self.broadcast.subscribe();

		let tracked = Arc::new(Mutex::new(HashSet::new()));

		{
			let tracked = tracked.clone();
			let server_tx = server_tx.clone();

			tokio::spawn(async move {
				while let Ok(message) = ipc_rx.recv().await {
					let mut tracked = tracked.lock().await;

					if !tracked.contains(message.icao()) {
						continue
					}

					if let Downstream::Error {
						icao,
						disconnect: true,
						..
					} = &message
					{
						debug_assert!(tracked.remove(icao));
						let _ = server_tx.send(Upstream::Track {
							icao: icao.clone(),
							track: false,
						});
					}

					if let Err(err) = stream_tx.send(message).await {
						debug!("{err}");
						break
					}
				}

				debug!("broadcast channel dropped");
			});
		}

		tokio::spawn(async move {
			loop {
				let message = match stream_rx.recv().await {
					Ok(message) => message,
					Err(_) => {
						let mut tracked = tracked.lock().await;

						for icao in tracked.drain() {
							let _ = server_tx.send(Upstream::Track { icao, track: false });
						}

						break
					},
				};

				match &message {
					Upstream::Init => continue,
					Upstream::Track { icao, track } => {
						let mut tracked = tracked.lock().await;

						if *track {
							if !tracked.insert(icao.clone()) {
								continue
							}
						} else {
							if !tracked.remove(icao) {
								continue
							}
						}
					},
					_ => (),
				}

				let _ = server_tx.send(message);
			}
		});

		Ok(())
	}
}

#[derive(Clone)]
struct AerodromeManager {
	data: Arc<Mutex<AerodromeManagerData>>,
	server: Option<(String, String)>,
	icao: String,
	broadcast: Sender<Downstream>,
}

struct AerodromeManagerData {
	config: Option<Aerodrome>,
	controlling: bool,
	trackers: usize,
	state: Patch,
	socket: Option<Arc<Mutex<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
}

impl AerodromeManager {
	async fn new(
		icao: &str,
		options: &Option<ConnectOptions>,
		config: Arc<Mutex<ConfigManager>>,
		broadcast: Sender<Downstream>,
	) -> Result<Self> {
		let this = Self {
			data: Arc::new(Mutex::new(AerodromeManagerData {
				config: None,
				controlling: false,
				trackers: 0,
				state: Patch::default(),
				socket: None,
			})),
			server: options.as_ref().map(|options| {
				let secure = options
					.server
					.parse::<Uri>()
					.ok()
					.map(|uri| {
						matches!(uri.scheme_str(), Some("https" | "wss"))
							|| uri.port_u16() == Some(443)
					})
					.unwrap_or_default();
				let base = options
					.server
					.split_once("://")
					.map(|s| s.1)
					.unwrap_or(&options.server)
					.trim_end_matches('/');

				(
					format!("{}://{base}", if secure { "s" } else { "" }),
					options.token.clone(),
				)
			}),
			icao: icao.into(),
			broadcast: broadcast.clone(),
		};

		{
			let icao = icao.to_string();
			let this = this.clone();
			tokio::spawn(async move {
				match config.lock().await.load(&icao).await {
					Ok(None) => (),
					Ok(Some(config)) => {
						{
							this.data.lock().await.config = Some(config);
						}
						this.sync_clients().await;
					},
					Err(err) => warn!("failed to load config: {err}"),
				}
			});
		}

		Ok(this)
	}

	fn broadcast(&self, message: Downstream) {
		if self.broadcast.send(message).is_err() {
			warn!("broadcast channel full");
		}
	}

	async fn sync_clients(&self) {
		let data = self.data.lock().await;
		if let Some(config) = &data.config {
			self.broadcast(Downstream::Config {
				data: config.clone(),
			});
			self.broadcast(Downstream::Control {
				icao: self.icao.clone(),
				control: data.controlling,
			});
			self.broadcast(Downstream::Patch {
				icao: self.icao.clone(),
				patch: data.state.clone(),
			});
		}
	}

	async fn connect(&self) -> Result<()> {
		let mut data = self.data.lock().await;

		if data.socket.is_some() {
			warn!("aerodrome connection attempted whilst connected");
			return Ok(())
		}

		if let Some((server, key)) = &self.server {
			let state_endpoint = format!("http{server}/state?airport={}", self.icao);
			let connect_endpoint =
				format!("ws{server}/connect?airport={}&key={}", self.icao, key);

			debug!(
				"connecting socket {}",
				connect_endpoint.rsplit_once("&key=").unwrap().0,
			);

			let socket = tokio_tungstenite::connect_async(connect_endpoint).await?.0;
			let socket = Arc::new(Mutex::new(socket));
			data.socket = Some(socket.clone());

			let socket = socket.clone();
			let this = self.clone();
			tokio::spawn(async move {
				use std::sync::atomic::{AtomicUsize, Ordering};
				static COUNTER: AtomicUsize = AtomicUsize::new(0);

				let mut last_state_poll = Instant::now();

				let n = COUNTER.fetch_add(1, Ordering::SeqCst);

				loop {
					let socket_arc = &socket;

					let mut socket = socket.lock().await;
					match tokio::time::timeout(SOCKET_POLL_TIMEOUT, socket.next()).await {
						Ok(Some(Ok(Message::Text(message)))) => {
							let Ok(data) = serde_json::from_str(message.as_str()) else {
								warn!("net downstream deserialisation failed");
								continue
							};

							trace!("ws rx ({n}): {data:?}");

							let res = match data {
								NetDownstream::Heartbeat => {
									Self::send(&mut socket, &NetUpstream::HeartbeatAck).await
								},
								NetDownstream::Close => {
									warn!("server-initiated graceful close");
									this
										.disconnect_forced(
											socket_arc,
											"server closed connection".into(),
										)
										.await;

									break
								},
								NetDownstream::Error { message } => {
									warn!("server: {message}");
									Ok(())
								},
								NetDownstream::InitialState { patch, .. }
								| NetDownstream::SharedStateUpdate { patch, .. } => {
									this.data.lock().await.state.apply_patch(patch.clone());
									this.broadcast(Downstream::Patch {
										icao: this.icao.clone(),
										patch,
									});
									Ok(())
								},
								NetDownstream::StateUpdate { .. }
								| NetDownstream::HeartbeatAck
								| NetDownstream::ControllerConnect { .. }
								| NetDownstream::ControllerDisconnect { .. }
								| NetDownstream::Other => Ok(()),
							};

							if let Err(err) = res {
								this
									.disconnect_forced(
										socket_arc,
										format!("server messaging error: {err}"),
									)
									.await;

								break
							}
						},
						Ok(Some(Ok(_))) => (),
						Ok(Some(Err(err))) => {
							warn!("socket closed with error: {err}");
							this
								.disconnect_forced(
									socket_arc,
									format!("server connection error: {err}"),
								)
								.await;

							break
						},
						Ok(None) => {
							debug!("socket closed");
							this
								.disconnect_forced(
									socket_arc,
									format!("connection closed unexpectedly"),
								)
								.await;

							break
						},
						Err(_) => {
							if last_state_poll.elapsed() > STATE_POLL_INTERVAL {
								debug!("interval poll state for {}", this.icao);

								last_state_poll = Instant::now();

								let response = match reqwest::get(&state_endpoint).await {
									Ok(response) => response,
									Err(err) => {
										warn!("failed to fetch state: {err}");
										continue
									},
								};

								let Ok(data) = response.json::<State>().await else {
									warn!("net state deserialisation failed");
									continue
								};

								this.broadcast(Downstream::Aircraft {
									icao: this.icao.clone(),
									aircraft: data.pilots,
								});
							}
						},
					}
				}
			});
		}

		Ok(())
	}

	async fn disconnect(&self) -> Result<()> {
		debug!("disconnecting socket");

		if let Some(socket) = &self.data.lock().await.socket.take() {
			let mut socket = socket.lock().await;

			Self::send(&mut socket, &NetUpstream::Close).await?;
			socket.close(None).await?;
		}

		Ok(())
	}

	async fn disconnect_forced(
		&self,
		socket_arc: &Arc<Mutex<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
		message: String,
	) {
		let mut data = self.data.lock().await;
		if data
			.socket
			.as_ref()
			.map(|socket| Arc::ptr_eq(socket, socket_arc))
			.unwrap_or_default()
		{
			data.socket = None;
			self.broadcast(Downstream::Error {
				icao: self.icao.clone(),
				message: Some(message),
				disconnect: true,
			});

			debug!("force-disconnected");
		} else {
			debug!("disconnect forced on redundant socket");
		}
	}

	async fn send(
		socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
		message: &NetUpstream,
	) -> Result<()> {
		trace!("ws tx: {message:?}");

		if let Ok(data) = serde_json::to_string(message) {
			socket.send(Message::Text(data.into())).await?;
		} else {
			#[cfg(debug_assertions)]
			panic!("net upstream serialisation failed");
		}

		Ok(())
	}

	async fn track(&self, track: bool) -> Result<()> {
		let trackers = {
			let mut data = self.data.lock().await;
			if track {
				data.trackers += 1;
			} else if data.trackers > 0 {
				data.trackers -= 1;
			}

			data.trackers
		};

		if trackers == 0 {
			debug!("tracking dropped {}: disconnecting", self.icao);
			self.disconnect().await?;
		} else if trackers == 1 && track {
			debug!("newly tracking {}: connecting", self.icao);
			if let Err(err) = self.connect().await {
				self.broadcast(Downstream::Error {
					icao: self.icao.clone(),
					message: Some(format!("server connection failed: {err}")),
					disconnect: true,
				});
				return Err(err)
			}
		}

		if track {
			self.sync_clients().await;
		}

		Ok(())
	}

	async fn control(&self, control: bool) {
		self.data.lock().await.controlling = control;
		self.broadcast(Downstream::Control {
			icao: self.icao.clone(),
			control,
		});
	}

	async fn patch(&self, patch: Patch) -> Result<()> {
		let mut data = self.data.lock().await;
		if let Some(socket) = &data.socket {
			let mut socket = socket.lock().await;
			Self::send(&mut socket, &NetUpstream::SharedStateUpdate { patch }).await
		} else {
			data.state.apply_patch(patch.clone());
			self.broadcast(Downstream::Patch {
				icao: self.icao.clone(),
				patch,
			});
			Ok(())
		}
	}

	async fn scenery(&self, scenery: HashMap<String, bool>) -> Result<()> {
		if let Some(socket) = &self.data.lock().await.socket {
			let mut socket = socket.lock().await;
			for (object_id, state) in scenery {
				let message = NetUpstream::StateUpdate { object_id, state };
				Self::send(&mut socket, &message).await?;
			}
		}

		Ok(())
	}
}
