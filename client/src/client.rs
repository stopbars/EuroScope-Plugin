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

		for (_, aerodrome) in &mut self.aerodromes {
			for message in aerodrome.pending() {
				self.channel.send(message)?;
			}
		}

		Ok(())
	}

	pub fn set_activity(
		&mut self,
		icao: String,
		state: ActivityState,
	) -> Result<()> {
		self.channel.send(Upstream::Activity { icao, state })
	}

	pub fn aerodrome(&self, icao: &String) -> Option<&Aerodrome> {
		self.aerodromes.get(icao)
	}

	pub fn aerodrome_mut(&mut self, icao: &String) -> Option<&mut Aerodrome> {
		self.aerodromes.get_mut(icao)
	}
}

pub struct Aerodrome {
	pending: Vec<Upstream>,

	config: bars_config::Aerodrome,
	state: ActivityState,

	profile: usize,

	node_ids: HashMap<String, usize>,
	block_ids: HashMap<String, usize>,

	nodes: Vec<State<bool>>,
	blocks: Vec<State<bars_config::BlockState>>,

	aircraft: HashSet<String>,
}

#[derive(Clone)]
struct State<T> {
	current: T,
	pending: Option<T>,
}

impl Aerodrome {
	fn new(config: bars_config::Aerodrome) -> Self {
		let mut this = Self {
			pending: Vec::new(),
			config,
			state: ActivityState::None,
			profile: 0,
			node_ids: HashMap::new(),
			block_ids: HashMap::new(),
			nodes: Vec::new(),
			blocks: Vec::new(),
			aircraft: HashSet::new(),
		};

		for (i, node) in this.config.nodes.iter().enumerate() {
			this.node_ids.insert(node.id.clone(), i);
		}
		for (i, block) in this.config.blocks.iter().enumerate() {
			this.block_ids.insert(block.id.clone(), i);
		}

		this.set_default_state();

		this
	}

	fn update_profile(&mut self, profile: String) {
		if let Some(i) = self.config.profiles.iter().position(|p| p.id == profile) {
			self.profile = i;
		}
	}

	fn pending(&mut self) -> Vec<Upstream> {
		std::mem::take(&mut self.pending)
	}

	fn bs_ipc_to_conf(
		&self,
		state: BlockState,
	) -> Option<bars_config::BlockState> {
		Some(match state {
			BlockState::Clear => bars_config::BlockState::Clear,
			BlockState::Relax => bars_config::BlockState::Relax,
			BlockState::Route((a, b)) => bars_config::BlockState::Route((
				*self.node_ids.get(&a)?,
				*self.node_ids.get(&b)?,
			)),
		})
	}

	fn bs_conf_to_ipc(&self, state: &bars_config::BlockState) -> BlockState {
		match state {
			bars_config::BlockState::Clear => BlockState::Clear,
			bars_config::BlockState::Relax => BlockState::Relax,
			bars_config::BlockState::Route((a, b)) => BlockState::Route((
				self.config.nodes[*a].id.clone(),
				self.config.nodes[*b].id.clone(),
			)),
		}
	}

	fn update_state(
		&mut self,
		nodes: HashMap<String, NodeState>,
		blocks: HashMap<String, BlockState>,
	) {
		for (id, state) in nodes {
			if let Some(i) = self.node_ids.get(&id).copied() {
				self.nodes[i].current = state;
				self.nodes[i].pending = None;
			}
		}

		for (id, state) in blocks {
			if let Some(i) = self.block_ids.get(&id).copied() {
				let Some(state) = self.bs_ipc_to_conf(state) else {
					continue
				};

				self.blocks[i].current = state;
				self.blocks[i].pending = None;
			}
		}
	}

	pub fn state(&self) -> ActivityState {
		self.state
	}

	pub fn profile(&self) -> usize {
		self.profile
	}

	pub fn set_profile(&mut self, i: usize) {
		if i >= self.config.profiles.len() {
			return
		}

		self.profile = i;

		self.pending.push(Upstream::Profile {
			icao: self.config.icao.clone(),
			profile: self.config.profiles[i].id.clone(),
		});
	}

	pub fn apply_preset(&mut self, i: usize) {
		if i >= self.config.profiles[self.profile].presets.len() {
			return
		}

		let preset = &self.config.profiles[self.profile].presets[i];
		let mut nodes = HashMap::new();
		let mut blocks = HashMap::new();

		for (node, state) in &preset.nodes {
			self.nodes[*node].pending = Some(*state);
			nodes.insert(self.config.nodes[*node].id.clone(), *state);
		}

		for (block, state) in &preset.blocks {
			self.blocks[*block].pending = Some(*state);
			blocks.insert(
				self.config.blocks[*block].id.clone(),
				self.bs_conf_to_ipc(state),
			);
		}

		self.pending.push(Upstream::State {
			icao: self.config.icao.clone(),
			nodes,
			blocks,
		});
	}

	pub fn config(&self) -> &bars_config::Aerodrome {
		&self.config
	}

	pub fn is_pilot_enabled(&self, callsign: &str) -> bool {
		self.aircraft.contains(callsign)
	}

	fn set_default_state(&mut self) {
		self.nodes = Vec::with_capacity(self.config.nodes.len());
		self.blocks = vec![
			State {
				current: bars_config::BlockState::Relax,
				pending: None,
			};
			self.config.blocks.len()
		];

		for i in 0..self.config.nodes.len() {
			self.nodes.push(State {
				current: match self.config.profiles[self.profile].nodes[i] {
					bars_config::NodeCondition::Fixed { state } => state,
					_ => true,
				},
				pending: None,
			});
		}
	}
}
