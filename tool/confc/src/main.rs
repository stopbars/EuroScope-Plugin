mod map;

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use bars_config::{ Config, Element };

use anyhow::Result;

use clap::Parser;

use serde::Deserialize;

/// Compile JSON files into a distributable BARS configuration package.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
	/// include NAME as the package name
	#[arg(short = 'n', long, value_name = "NAME")]
	pkg_name: Option<String>,

	/// include VERSION as the package version
	#[arg(short = 'v', long, value_name = "VERSION")]
	pkg_version: Option<String>,

	/// write output to FILE
	#[arg(short, long, value_name = "FILE")]
	output: Option<PathBuf>,

	/// paths to JSON files to process
	#[arg(value_name = "FILE")]
	files: Vec<PathBuf>,
}

fn main() -> Result<()> {
	let args = Args::parse();

	for file in args.files {
		let s = std::fs::read_to_string(file)?;
		let _ = serde_json::from_str::<Aerodrome>(&s)?;
	}

	let config = Config {
		name: args.pkg_name,
		version: args.pkg_version,
		aerodromes: Vec::new(),
	};

	if let Some(path) = args.output {
		config.save(BufWriter::new(File::create(path)?))?;
	} else {
		config.save(std::io::stdout())?;
	}

	Ok(())
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(transparent)]
struct Id(String);

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(from = "&str")]
struct IdList(Vec<Id>);

impl From<&str> for IdList {
	fn from(s: &str) -> Self {
		if s.is_empty() {
			Self(Vec::new())
		} else {
			Self(s.split('+').map(|s| Id(s.to_string())).collect())
		}
	}
}

#[derive(Debug, Deserialize)]
pub struct Aerodrome {
	icao: String,

	elements: Vec<Element>,
	nodes: Vec<Node>,
	#[serde(default)]
	edges: Vec<Edge>,
	#[serde(default)]
	blocks: Vec<Block>,

	#[serde(default)]
	profiles: Vec<Profile>,

	display: GeoMap,
	#[serde(default)]
	maps: Vec<Map>,
}

#[derive(Debug, Deserialize)]
struct Node {
	id: Id,

	scratchpad: Option<String>,
	parent: Option<Id>,
}

#[derive(Debug, Deserialize)]
struct Edge {
	id: Id,
}

#[derive(Debug, Deserialize)]
struct Block {
	id: Id,

	nodes: Vec<Id>,
	edges: HashMap<Id, IdList>,
	#[serde(default)]
	joins: Vec<Vec<IdList>>,

	#[serde(default)]
	stands: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Profile {
	id: Id,
	name: String,

	#[serde(default)]
	nodes: HashMap<IdList, NodeCondition>,
	#[serde(default)]
	edges: HashMap<IdList, EdgeCondition>,
	#[serde(default)]
	blocks: HashMap<IdList, BlockCondition>,

	#[serde(default)]
	presets: Vec<Preset>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum NodeCondition {
	Fixed {
		state: bool,
	},
	Direct {
		timer: ResetCondition,
	},
	Router,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum EdgeCondition {
	Fixed {
		state: bool,
	},
	Direct {
		node: Id,
	},
	Router,
}

#[derive(Debug, Deserialize)]
struct BlockCondition {
	timer: ResetCondition,
}

type ResetCondition = Option<u32>;

#[derive(Debug, Deserialize)]
struct Preset {
	name: String,

	#[serde(default)]
	nodes: HashMap<IdList, NodeState>,
	#[serde(default)]
	blocks: HashMap<IdList, BlockState>,
}

type NodeState = bool;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BlockState {
	Clear,
	Relax,
	#[serde(untagged)]
	Route((Id, Id)),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GeoMap {
	Geo(PathBuf),
	Flat {
		svg: PathBuf,
		lat: (f64, f64),
		lon: (f64, f64),
	},
}

type Map = PathBuf;
