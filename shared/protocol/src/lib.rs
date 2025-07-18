use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub type NodeState = bool;

#[derive(
	Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum BlockState {
	Clear,
	Relax,
	Route((String, String)),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Aerodrome {
	pub profile: String,
	pub nodes: HashMap<String, NodeState>,
	pub blocks: HashMap<String, BlockState>,
	patch: Option<Patch>,
}

impl Aerodrome {
	pub fn new(profile: String) -> Self {
		Self {
			profile,
			nodes: HashMap::new(),
			blocks: HashMap::new(),
			patch: None,
		}
	}

	fn patch(&mut self) -> &mut Patch {
		self.patch.get_or_insert_default()
	}

	pub fn set_profile(&mut self, profile: String) {
		self.patch().profile = Some(profile.clone());
		self.profile = profile;
	}

	pub fn set_node(&mut self, id: String, state: NodeState) {
		self.patch().nodes.insert(id.clone(), state);
		self.nodes.insert(id, state);
	}

	pub fn set_block(&mut self, id: String, state: BlockState) {
		self.patch().blocks.insert(id.clone(), state.clone());
		self.blocks.insert(id, state);
	}

	pub fn take_patch(&mut self) -> Option<Patch> {
		std::mem::take(&mut self.patch)
	}

	pub fn apply_patch(&mut self, patch: Patch) {
		if let Some(profile) = patch.profile {
			self.profile = profile;
		}

		self.nodes.extend(patch.nodes.into_iter());
		self.blocks.extend(patch.blocks.into_iter());
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Patch {
	pub profile: Option<String>,
	pub nodes: HashMap<String, NodeState>,
	pub blocks: HashMap<String, BlockState>,
}

impl Patch {
	pub fn apply_patch(&mut self, patch: Patch) {
		if let Some(profile) = patch.profile {
			self.profile = Some(profile);
		}

		self.nodes.extend(patch.nodes.into_iter());
		self.blocks.extend(patch.blocks.into_iter());
	}

	pub fn is_empty(&self) -> bool {
		self.profile.is_none() && self.nodes.is_empty() && self.blocks.is_empty()
	}
}

impl Default for Patch {
	fn default() -> Self {
		Self {
			profile: None,
			nodes: HashMap::new(),
			blocks: HashMap::new(),
		}
	}
}

impl From<Aerodrome> for Patch {
	fn from(from: Aerodrome) -> Self {
		Self {
			profile: Some(from.profile),
			nodes: from.nodes,
			blocks: from.blocks,
		}
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
	rename_all = "SCREAMING_SNAKE_CASE",
	rename_all_fields = "camelCase",
	tag = "type",
	content = "data"
)]
pub enum Upstream<P = Patch> {
	Heartbeat,
	HeartbeatAck,
	Close,
	StateUpdate {
		object_id: String,
		state: bool,
	},
	SharedStateUpdate {
		#[serde(rename = "sharedStatePatch")]
		patch: P,
	},
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
	rename_all = "SCREAMING_SNAKE_CASE",
	rename_all_fields = "camelCase",
	tag = "type",
	content = "data"
)]
pub enum Downstream<P = Patch> {
	Heartbeat,
	HeartbeatAck,
	Close,
	Error {
		message: String,
	},
	ControllerConnect {
		controller_id: String,
	},
	ControllerDisconnect {
		controller_id: String,
	},
	InitialState {
		connection_type: String,
		#[serde(rename = "objects")]
		scenery: Vec<SceneryObject>,
		#[serde(rename = "sharedState")]
		patch: P,
	},
	StateUpdate {
		object_id: String,
		state: bool,
		controller_id: String,
	},
	SharedStateUpdate {
		#[serde(rename = "sharedStatePatch")]
		patch: P,
		controller_id: String,
	},
	#[serde(other)]
	Other,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SceneryObject {
	pub id: String,
	pub state: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct State {
	pub airport: String,
	pub controllers: Vec<String>,
	pub pilots: Vec<String>,
	pub offline: bool,
}
