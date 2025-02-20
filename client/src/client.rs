use crate::ipc::{Channel, Downstream, Upstream};
use crate::ActivityState;

use std::collections::{HashMap, HashSet};

use bars_config::BlockState;

use bars_protocol::{BlockState as IpcBlockState, Patch};

use anyhow::Result;

use tracing::warn;

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
				Downstream::Control { icao, control } => {
					if let Some(aerodrome) = self.aerodromes.get_mut(&icao) {
						aerodrome.state = if control {
							ActivityState::Controlling
						} else {
							ActivityState::Observing
						};
					}
				},
				Downstream::Patch { icao, patch } => {
					if let Some(aerodrome) = self.aerodromes.get_mut(&icao) {
						aerodrome.apply_patch(patch);
					}
				},
				Downstream::Aircraft { icao, aircraft } => {
					if let Some(aerodrome) = self.aerodromes.get_mut(&icao) {
						aerodrome.aircraft = HashSet::from_iter(aircraft);
					}
				},
			}
		}

		for (icao, aerodrome) in &mut self.aerodromes {
			let patch = std::mem::take(&mut aerodrome.pending_patch);
			if !patch.is_empty() {
				self.channel.send(Upstream::Patch {
					icao: icao.clone(),
					patch,
				})?;
			}

			let scenery = std::mem::take(&mut aerodrome.pending_scenery);
			if !scenery.is_empty() {
				self.channel.send(Upstream::Scenery {
					icao: icao.clone(),
					scenery,
				})?;
			}
		}

		Ok(())
	}

	pub fn set_tracking(&mut self, icao: String, track: bool) -> Result<()> {
		if !track {
			self.aerodromes.remove(&icao);
		}

		self.channel.send(Upstream::Track { icao, track })
	}

	pub fn set_controlling(&mut self, icao: String, control: bool) -> Result<()> {
		if self.aerodromes.contains_key(&icao) {
			self.channel.send(Upstream::Control { icao, control })
		} else {
			warn!("attempted to un/control untracked aerodrome");
			Ok(())
		}
	}

	pub fn aerodrome(&self, icao: &String) -> Option<&Aerodrome> {
		self.aerodromes.get(icao)
	}

	pub fn aerodrome_mut(&mut self, icao: &String) -> Option<&mut Aerodrome> {
		self.aerodromes.get_mut(icao)
	}
}

pub struct Aerodrome {
	config: bars_config::Aerodrome,
	state: ActivityState,

	profile: usize,

	node_ids: HashMap<String, usize>,
	block_ids: HashMap<String, usize>,

	nodes: Vec<State<bool>>,
	blocks: Vec<State<BlockState>>,

	aircraft: HashSet<String>,

	pending_patch: Patch,
	pending_scenery: HashMap<String, bool>,
}

#[derive(Clone)]
struct State<T> {
	current: T,
	pending: Option<T>,
}

impl Aerodrome {
	fn new(config: bars_config::Aerodrome) -> Self {
		let mut this = Self {
			config,
			state: ActivityState::None,
			profile: 0,
			node_ids: HashMap::new(),
			block_ids: HashMap::new(),
			nodes: Vec::new(),
			blocks: Vec::new(),
			aircraft: HashSet::new(),
			pending_patch: Default::default(),
			pending_scenery: HashMap::new(),
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

	fn bs_ipc_to_conf(&self, state: IpcBlockState) -> Option<BlockState> {
		Some(match state {
			IpcBlockState::Clear => BlockState::Clear,
			IpcBlockState::Relax => BlockState::Relax,
			IpcBlockState::Route((a, b)) => {
				BlockState::Route((*self.node_ids.get(&a)?, *self.node_ids.get(&b)?))
			},
		})
	}

	fn bs_conf_to_ipc(&self, state: &BlockState) -> IpcBlockState {
		match state {
			BlockState::Clear => IpcBlockState::Clear,
			BlockState::Relax => IpcBlockState::Relax,
			BlockState::Route((a, b)) => IpcBlockState::Route((
				self.config.nodes[*a].id.clone(),
				self.config.nodes[*b].id.clone(),
			)),
		}
	}

	fn apply_patch(&mut self, patch: Patch) {
		if let Some(profile) = patch.profile {
			if let Some(i) = self.config.profiles.iter().position(|p| p.id == profile)
			{
				self.profile = i;
			} else {
				warn!("requested to set unknown profile");
			}
		}

		for (id, state) in patch.nodes {
			if let Some(i) = self.node_ids.get(&id).copied() {
				self.nodes[i].current = state;
				self.nodes[i].pending = None;
			}
		}

		for (id, state) in patch.blocks {
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
		self.pending_patch.profile = Some(self.config.profiles[i].id.clone());
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

		self.pending_patch.nodes = nodes;
		self.pending_patch.blocks = blocks;
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
