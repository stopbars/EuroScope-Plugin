use crate::ipc::{BlockState, Channel, Downstream, NodeState, Upstream};
use crate::ActivityState;

use std::collections::{HashMap, HashSet};

use anyhow::Result;

pub struct Client {
	channel: Channel,
	aerodromes: HashMap<String, Aerodrome>,
}

impl Client {
	pub fn new(mut channel: Channel) -> Result<Self> {
		channel.send(Upstream::Init)?;

		Ok(Self {
			channel,
			aerodromes: HashMap::new(),
		})
	}

	pub fn disconnect(self) {}

	pub fn tick(&mut self) -> Result<()> {
		while let Some(message) = self.channel.recv()? {
			match message {
				Downstream::Config { data } => {
					self
						.aerodromes
						.insert(data.icao.clone(), Aerodrome::new(data));
				},
				Downstream::Activity { icao, state } => {
					if let Some(aerodrome) = self.aerodromes.get_mut(&icao) {
						aerodrome.state = state;
					}
				},
				Downstream::Profile { icao, profile } => {
					if let Some(aerodrome) = self.aerodromes.get_mut(&icao) {
						aerodrome.update_profile(profile);
					}
				},
				Downstream::State {
					icao,
					nodes,
					blocks,
				} => {
					if let Some(aerodrome) = self.aerodromes.get_mut(&icao) {
						aerodrome.update_state(nodes, blocks);
					}
				},
				Downstream::Aircraft { icao, aircraft } => {
					if let Some(aerodrome) = self.aerodromes.get_mut(&icao) {
						aerodrome.aircraft = HashSet::from_iter(aircraft);
					}
				},
			}
		}

		Ok(())
	}
}

pub struct Aerodrome {
	config: bars_config::Aerodrome,
	state: ActivityState,
	profile: Option<String>,
	nodes: Vec<NodeState>,
	blocks: Vec<BlockState>,
	aircraft: HashSet<String>,
}

impl Aerodrome {
	fn new(config: bars_config::Aerodrome) -> Self {
		Self {
			config,
			state: ActivityState::None,
			profile: None,
			nodes: Vec::new(),
			blocks: Vec::new(),
			aircraft: HashSet::new(),
		}
	}

	fn update_profile(&mut self, profile: String) {
		todo!()
	}

	fn update_state(
		&mut self,
		nodes: HashMap<String, NodeState>,
		blocks: HashMap<String, BlockState>,
	) {
		todo!()
	}
}
