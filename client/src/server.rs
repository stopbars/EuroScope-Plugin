use crate::ipc::{Channel, Downstream, ServerChannel, Upstream};

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
	pub fn new(connect: Option<ConnectOptions>) -> Result<(Self, Channel)> {
		let (channel, server_channel) = crate::ipc::mpsc_pair();

		let runtime = RuntimeBuilder::new_current_thread().enable_io().build()?;

		let (shutdown, srx) = tokio::sync::oneshot::channel();
		let (ctx, cancelled) = tokio::sync::oneshot::channel();

		let thread =
			ThreadBuilder::new().name("server".into()).spawn(move || {
				runtime.block_on(async {
					debug!("worker thread spawned");

					if let Err(err) = Worker::run(connect, server_channel).await {
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

#[derive(Clone)]
struct Worker {
	clients: Arc<Mutex<Vec<UnboundedSender<Downstream>>>>,
	upstreams: UnboundedSender<Upstream>,
}

impl Worker {
	async fn run(
		connect: Option<ConnectOptions>,
		channel: ServerChannel,
	) -> Result<()> {
		let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

		Self {
			clients: Arc::new(Mutex::new(Vec::new())),
			upstreams: tx,
		}
		.run_impl(connect, channel, rx)
		.await
	}

	async fn run_impl(
		&mut self,
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

		let mut state = self.clone();
		tokio::spawn(async move {
			if let Err(err) = state.handle_channel(channel).await {
				debug!("{err}");
			}
		});

		Ok(())
	}

	async fn bind(&mut self, port: u16) -> Result<()> {
		let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;

		let state = self.clone();

		tokio::spawn(async move {
			loop {
				if let Ok((stream, remote)) = listener.accept().await {
					debug!("accepted {remote}");

					let mut state = state.clone();
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
		&mut self,
		rx: UnboundedReceiver<Upstream>,
		_options: ConnectOptions,
	) -> Result<()> {
		// TODO
		self.handle_local(rx).await
	}

	async fn handle_local(
		&mut self,
		mut rx: UnboundedReceiver<Upstream>,
	) -> Result<()> {
		tokio::spawn(async move {
			while let Some(message) = rx.recv().await {
				match message {
					_ => warn!("unknown message forwarded to local handler"),
				}
			}
		});

		Ok(())
	}

	async fn handle_channel(&mut self, stream: ServerChannel) -> Result<()> {
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
				Upstream::Init => (),
			}
		}
	}

	async fn broadcast(&mut self, message: Downstream) {
		self
			.clients
			.lock()
			.await
			.retain(|tx| tx.send(message.clone()).is_ok());
	}
}
