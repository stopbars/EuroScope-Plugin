use crate::ipc::{Channel, Upstream};

use anyhow::Result;

pub struct Client {
	channel: Channel,
}

impl Client {
	pub fn new(mut channel: Channel) -> Result<Self> {
		channel.send(Upstream::Init)?;

		Ok(Self { channel })
	}

	pub fn disconnect(self) {}

	pub fn tick(&mut self) -> Result<()> {
		while let Some(message) = self.channel.recv()? {}

		Ok(())
	}
}
