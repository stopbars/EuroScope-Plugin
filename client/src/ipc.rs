use std::collections::HashMap;
use std::io::{ErrorKind, Write};
use std::net::{Ipv4Addr, TcpStream};

use bars_protocol::Patch;

use anyhow::{bail, Result};

use serde::{Deserialize, Serialize};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream as AsyncTcpStream;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use tracing::trace;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Upstream {
	Init,
	Track {
		icao: String,
		track: bool,
	},
	Control {
		icao: String,
		control: bool,
	},
	Patch {
		icao: String,
		patch: Patch,
	},
	Scenery {
		icao: String,
		scenery: HashMap<String, bool>,
	},
}

impl Upstream {
	pub fn icao(&self) -> Option<&String> {
		Some(match self {
			Self::Track { icao, .. } => icao,
			Self::Control { icao, .. } => icao,
			Self::Patch { icao, .. } => icao,
			Self::Scenery { icao, .. } => icao,
			_ => return None,
		})
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Downstream {
	Config {
		data: bars_config::Aerodrome,
	},
	Control {
		icao: String,
		control: bool,
	},
	Patch {
		icao: String,
		patch: Patch,
	},
	Aircraft {
		icao: String,
		aircraft: Vec<String>,
	},
	Error {
		icao: String,
		message: Option<String>,
		disconnect: bool,
	},
}

impl Downstream {
	pub fn icao(&self) -> &String {
		match self {
			Self::Config { data } => &data.icao,
			Self::Control { icao, .. } => icao,
			Self::Patch { icao, .. } => icao,
			Self::Aircraft { icao, .. } => icao,
			Self::Error { icao, .. } => icao,
		}
	}
}

pub enum Channel {
	Mpsc {
		rx: UnboundedReceiver<Downstream>,
		tx: UnboundedSender<Upstream>,
	},
	Tcp(TcpStream),
}

impl Channel {
	pub fn connect(port: u16) -> Result<Self> {
		let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))?;
		stream.set_nonblocking(true)?;
		Ok(Self::Tcp(stream))
	}

	pub fn send(&mut self, message: Upstream) -> Result<()> {
		trace!("cch tx: {message:?}");

		match self {
			Self::Mpsc { tx, .. } => {
				tx.send(message)?;
			},
			Self::Tcp(stream) => {
				let n = bincode::serialized_size(&message)? as u32;
				stream.write_all(&n.to_le_bytes())?;
				bincode::serialize_into(stream, &message)?;
			},
		}

		Ok(())
	}

	pub fn recv(&mut self) -> Result<Option<Downstream>> {
		match self {
			Self::Mpsc { rx, .. } => match rx.try_recv() {
				Ok(message) => {
					trace!("cch rx: {message:?}");
					Ok(Some(message))
				},
				Err(TryRecvError::Empty) => Ok(None),
				Err(_) => bail!("disconnected"),
			},
			Self::Tcp(stream) => {
				let mut buf = [0];
				match stream.peek(&mut buf) {
					Ok(0) => return Ok(None),
					Ok(_) => (),
					Err(err) if err.kind() == ErrorKind::WouldBlock => return Ok(None),
					Err(err) => return Err(err.into()),
				}

				let message = bincode::deserialize_from(stream)?;
				trace!("cch rx: {message:?}");
				Ok(Some(message))
			},
		}
	}
}

pub enum ServerChannel {
	Mpsc {
		rx: UnboundedReceiver<Upstream>,
		tx: UnboundedSender<Downstream>,
	},
	Tcp(AsyncTcpStream),
}

impl ServerChannel {
	/* pub async fn send(&mut self, message: Downstream) -> Result<()> {
		match self {
			Self::Mpsc { tx, .. } => Self::send_mpsc(tx, message).await,
			Self::Tcp(stream) => Self::send_tcp(stream, message).await,
		}
	} */

	async fn send_mpsc(
		tx: &mut UnboundedSender<Downstream>,
		message: Downstream,
	) -> Result<()> {
		tx.send(message)?;
		Ok(())
	}

	async fn send_tcp<T: AsyncWriteExt + Unpin>(
		tx: &mut T,
		message: Downstream,
	) -> Result<()> {
		let data = bincode::serialize(&message)?;
		tx.write_all(&data).await?;
		Ok(())
	}

	/* pub async fn recv(&mut self) -> Result<Upstream> {
		match self {
			Self::Mpsc { rx, .. } => Self::recv_mpsc(rx).await,
			Self::Tcp(stream) => {
				stream.readable().await?;
				Self::recv_tcp(stream).await
			},
		}
	} */

	async fn recv_mpsc(rx: &mut UnboundedReceiver<Upstream>) -> Result<Upstream> {
		match rx.recv().await {
			Some(message) => Ok(message),
			None => bail!("disconnected"),
		}
	}

	async fn recv_tcp<T: AsyncReadExt + Unpin>(rx: &mut T) -> Result<Upstream> {
		let n = rx.read_u32_le().await?;
		if n > 0x100_0000 {
			bail!("oversized packet");
		} else {
			let mut buf = vec![0; n as usize];
			rx.read_exact(&mut buf).await?;
			Ok(bincode::deserialize(&buf)?)
		}
	}

	pub fn into_split(self) -> (ServerChannelReadHalf, ServerChannelWriteHalf) {
		match self {
			Self::Mpsc { rx, tx } => (
				ServerChannelReadHalf::Mpsc(rx),
				ServerChannelWriteHalf::Mpsc(tx),
			),
			Self::Tcp(stream) => {
				let (rx, tx) = stream.into_split();
				(
					ServerChannelReadHalf::Tcp(rx),
					ServerChannelWriteHalf::Tcp(tx),
				)
			},
		}
	}
}

pub enum ServerChannelReadHalf {
	Mpsc(UnboundedReceiver<Upstream>),
	Tcp(OwnedReadHalf),
}

impl ServerChannelReadHalf {
	pub async fn recv(&mut self) -> Result<Upstream> {
		let message = match self {
			Self::Mpsc(rx) => ServerChannel::recv_mpsc(rx).await,
			Self::Tcp(rx) => {
				rx.readable().await?;
				ServerChannel::recv_tcp(rx).await
			},
		}?;
		trace!("sch rx: {message:?}");
		Ok(message)
	}
}

pub enum ServerChannelWriteHalf {
	Mpsc(UnboundedSender<Downstream>),
	Tcp(OwnedWriteHalf),
}

impl ServerChannelWriteHalf {
	pub async fn send(&mut self, message: Downstream) -> Result<()> {
		trace!("sch tx: {message:?}");

		match self {
			Self::Mpsc(tx) => ServerChannel::send_mpsc(tx, message).await,
			Self::Tcp(tx) => ServerChannel::send_tcp(tx, message).await,
		}
	}
}

pub fn mpsc_pair() -> (Channel, ServerChannel) {
	let (utx, urx) = mpsc::unbounded_channel();
	let (dtx, drx) = mpsc::unbounded_channel();

	(
		Channel::Mpsc { rx: drx, tx: utx },
		ServerChannel::Mpsc { rx: urx, tx: dtx },
	)
}
