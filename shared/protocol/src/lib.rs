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

#[derive(Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum Upstream {
	#[serde(rename = "sub")]
	Subscribe {
		icao: String,
		#[serde(rename = "sub")]
		subscribe: bool,
		#[serde(rename = "ctl")]
		control: bool,
		#[serde(rename = "ext")]
		extended: bool,
	},
	#[serde(rename = "set")]
	Scenery {
		icao: String,
		#[serde(rename = "set")]
		scenery: HashMap<String, bool>,
	},
	#[serde(rename = "syn")]
	Patch {
		icao: String,
		#[serde(rename = "syn")]
		patch: Patch,
	},
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum Downstream {
	#[serde(rename = "syn")]
	Patch {
		icao: String,
		#[serde(rename = "syn")]
		patch: Patch,
		#[serde(rename = "self")]
		loopback: bool,
	},
	#[serde(rename = "epl")]
	Aircraft {
		icao: String,
		#[serde(rename = "epl")]
		aircraft: Vec<String>,
	},
	#[serde(other)]
	Other,
}
