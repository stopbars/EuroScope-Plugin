mod map;

use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use bars_config::{self as lib, Config, Element};

use anyhow::Result;

use clap::Parser;

use kml::KmlReader;

use serde::Deserialize;

use usvg::Tree;

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

	let mut aerodromes = Vec::new();

	for file in args.files {
		let dir = file.parent().unwrap();

		let s = std::fs::read_to_string(&file)?;
		let input = serde_json::from_str::<Aerodrome>(&s)?;

		let mut display = match input.display {
			GeoMap::Geo(path) => {
				let mut reader = KmlReader::<_, f32>::from_kmz_path(dir.join(path))?;
				map::convert(map::Kml::new(reader.read()?).unwrap().input(), 0)
			},
			GeoMap::Flat { svg, lat, lon } => {
				let s = std::fs::read_to_string(dir.join(svg))?;
				let tree = Tree::from_str(&s, &Default::default())?;
				map::convert(map::GeoSvg::new(&tree, lat, lon), 0)
			},
		};
		let mut styles = display.styles;

		let mut temp_maps = Vec::new();
		for svg in input.maps {
			let s = std::fs::read_to_string(dir.join(svg))?;
			let tree = Tree::from_str(&s, &Default::default())?;
			let mut map = map::convert(map::Svg::new(&tree), styles.len());
			styles.append(&mut map.styles);
			temp_maps.push(map);
		}

		let mut nodes = Vec::new();
		let mut node_ids = HashMap::new();
		for node in input.nodes {
			let parent = node.parent.map(|id| *node_ids.get(&id).unwrap());
			let display = display.nodes.remove(&node.id).unwrap_or_default();

			node_ids.insert(node.id.clone(), nodes.len());
			nodes.push(lib::Node {
				id: node.id.0,
				scratchpad: node.scratchpad,
				parent,
				display,
			});
		}

		let mut edges = Vec::new();
		let mut edge_ids = HashMap::new();
		for edge in input.edges {
			let display = display.edges.remove(&edge.id).unwrap_or_default();

			edge_ids.insert(edge.id, edges.len());
			edges.push(lib::Edge { display });
		}

		let mut edge_conditions = HashMap::new();
		let mut edge_blocks = HashMap::new();

		let mut blocks = Vec::new();
		let mut block_ids = HashMap::new();
		for block in input.blocks {
			let edges = HashMap::from_iter(block.edges.iter().map(|(id, edges)| {
				(
					*node_ids.get(id).unwrap(),
					edges
						.0
						.iter()
						.map(|id| *edge_ids.get(id).unwrap())
						.collect(),
				)
			}));
			let joins = block
				.joins
				.iter()
				.map(|vertex| {
					vertex
						.iter()
						.map(|edges| {
							edges
								.0
								.iter()
								.map(|id| *edge_ids.get(id).unwrap())
								.collect()
						})
						.collect()
				})
				.collect();

			let resolved = resolve_routes(&edges, &joins);
			for id in resolved.conditions.keys() {
				edge_blocks.insert(*id, blocks.len());
			}
			edge_conditions.extend(resolved.conditions.into_iter());

			let nodes = block
				.nodes
				.iter()
				.map(|id| *node_ids.get(id).unwrap())
				.collect();
			let display = display.blocks.remove(&block.id).unwrap_or_default();

			block_ids.insert(block.id.clone(), blocks.len());
			blocks.push(lib::Block {
				id: block.id.0,
				nodes,
				edges: Vec::new(), // defect: unused
				non_routes: resolved.non_routes,
				stands: block.stands,
				display,
			});
		}

		let mut profiles = Vec::new();
		for profile in input.profiles {
			let default_node = profile
				.nodes
				.get(&IdList::wildcard())
				.copied()
				.unwrap_or_default();
			let nodes = nodes
				.iter()
				.map(|node| {
					profile
						.nodes
						.iter()
						.find(|(ids, _)| ids.0.contains(&Id(node.id.clone())))
						.map(|(_, node)| *node)
						.unwrap_or(default_node)
						.convert()
				})
				.collect();

			let default_edge = profile
				.edges
				.get(&IdList::wildcard())
				.cloned()
				.unwrap_or_default();
			let edges = edge_ids
				.iter()
				.map(|(id, index)| {
					profile
						.edges
						.iter()
						.find(|(ids, _)| ids.0.contains(id))
						.map(|(_, edge)| edge.clone())
						.unwrap_or(default_edge.clone())
						.convert(
							&node_ids,
							edge_blocks
								.get(index)
								.copied()
								.zip(edge_conditions.get(index).cloned()),
						)
				})
				.collect();

			let default_block = profile
				.blocks
				.get(&IdList::wildcard())
				.copied()
				.unwrap_or_default();
			let blocks = blocks
				.iter()
				.map(|block| {
					profile
						.blocks
						.iter()
						.find(|(ids, _)| ids.0.contains(&Id(block.id.clone())))
						.map(|(_, block)| *block)
						.unwrap_or(default_block)
						.convert()
				})
				.collect();

			let presets = profile
				.presets
				.into_iter()
				.map(|preset| lib::Preset {
					name: preset.name,
					nodes: preset
						.nodes
						.into_iter()
						.flat_map(|(ids, state)| {
							ids
								.0
								.iter()
								.map(|id| *node_ids.get(id).unwrap())
								.map(move |index| (index, state.clone()))
								.collect::<Vec<_>>()
						})
						.collect(),
					blocks: preset
						.blocks
						.into_iter()
						.flat_map(|(ids, state)| {
							let state = match state {
								BlockState::Clear => lib::BlockState::Clear,
								BlockState::Relax => lib::BlockState::Relax,
								BlockState::Route((a, b)) => lib::BlockState::Route((
									*node_ids.get(&a).unwrap(),
									*node_ids.get(&b).unwrap(),
								)),
							};

							ids
								.0
								.into_iter()
								.map(|id| *block_ids.get(&id).unwrap())
								.map(move |index| (index, state))
						})
						.collect(),
				})
				.collect();

			profiles.push(lib::Profile {
				id: profile.id.0,
				name: profile.name,
				nodes,
				edges,
				blocks,
				presets,
			});
		}

		let mut maps = Vec::new();
		let mut views = Vec::new();
		for map in temp_maps {
			let mut nodes = vec![Default::default(); nodes.len()];
			for (id, node) in map.nodes {
				nodes[*node_ids.get(&id).unwrap()] = node;
			}

			let mut edges = vec![Default::default(); edges.len()];
			for (id, edge) in map.edges {
				edges[*edge_ids.get(&id).unwrap()] = edge;
			}

			let mut blocks = vec![Default::default(); blocks.len()];
			for (id, block) in map.blocks {
				blocks[*block_ids.get(&id).unwrap()] = block;
			}

			for (name, (min, max)) in map.views {
				views.push(lib::View {
					name,
					map: maps.len(),
					bounds: lib::Box { min, max },
				});
			}

			maps.push(lib::Map {
				background: Default::default(), // todo
				base: map.base,
				nodes,
				edges,
				blocks,
			});
		}

		aerodromes.push(lib::Aerodrome {
			icao: input.icao,
			elements: input.elements,
			nodes,
			edges,
			blocks,
			profiles,
			maps,
			views,
			styles,
		});
	}

	let config = Config {
		name: args.pkg_name,
		version: args.pkg_version,
		aerodromes,
	};

	if let Some(path) = args.output {
		config.save(BufWriter::new(File::create(path)?))?;
	} else {
		config.save(std::io::stdout())?;
	}

	Ok(())
}

fn resolve_routes(
	edges: &HashMap<usize, Vec<usize>>,
	joins: &Vec<Vec<Vec<usize>>>,
) -> Resolved {
	let mut conn1 = HashMap::new();
	let mut conn2 = HashMap::new();

	for vertex in joins {
		for (i, sector1) in vertex.iter().enumerate() {
			let mut edges = Vec::new();

			for (j, sector2) in vertex.iter().enumerate() {
				if i == j {
					continue
				}

				for edge in sector2 {
					edges.push(*edge);
				}
			}

			for edge in sector1 {
				if conn1.contains_key(&edge) {
					conn2.insert(edge, edges.clone());
				} else {
					conn1.insert(edge, edges.clone());
				}
			}
		}
	}

	let mut non_routes = Vec::new();
	let mut conditions = HashMap::<usize, Vec<(usize, usize)>>::new();

	for node1 in edges.keys() {
		'pairs: for node2 in edges.keys() {
			if node1 >= node2 {
				continue
			}

			let target = edges.get(node2).unwrap();

			let mut queue = VecDeque::from_iter(
				edges.get(node1).unwrap().iter().map(|k| (k, None)),
			);
			let mut prev = HashMap::<usize, usize>::new();

			while let Some((edge, last)) = queue.pop_front() {
				if let Some(last) = last {
					if prev.contains_key(edge) {
						continue
					} else {
						prev.insert(*edge, last);
					}
				}

				if target.contains(edge) {
					let mut edge = Some(edge);
					while let Some(this) = edge {
						conditions.entry(*this).or_default().push((*node1, *node2));

						edge = prev.get(this);
					}

					continue 'pairs
				}

				if let Some(c1) = conn1.get(edge) {
					let an = if last.map(|last| !c1.contains(&last)).unwrap_or(true) {
						c1
					} else if let Some(c2) = conn2.get(edge) {
						c2
					} else {
						continue
					};

					for next in an {
						queue.push_back((next, Some(*edge)));
					}
				} else {
					eprintln!("warning: boundary edge with no connection");
				}
			}

			non_routes.push((*node1, *node2));
		}
	}

	Resolved {
		non_routes,
		conditions,
	}
}

#[derive(Debug)]
struct Resolved {
	non_routes: Vec<(usize, usize)>,
	conditions: HashMap<usize, Vec<(usize, usize)>>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(transparent)]
struct Id(String);

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(from = "&str")]
struct IdList(Vec<Id>);

impl IdList {
	fn wildcard() -> Self {
		Self(Vec::new())
	}
}

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

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum NodeCondition {
	Fixed { state: bool },
	Direct { timer: ResetCondition },
	Router,
}

impl NodeCondition {
	fn convert(self) -> lib::NodeCondition {
		match self {
			Self::Fixed { state } => lib::NodeCondition::Fixed { state },
			Self::Direct { timer } => lib::NodeCondition::Direct {
				reset: timer
					.map(|t| lib::ResetCondition::TimeSecs(t))
					.unwrap_or(lib::ResetCondition::None),
			},
			Self::Router => lib::NodeCondition::Router,
		}
	}
}

impl Default for NodeCondition {
	fn default() -> Self {
		Self::Fixed { state: false }
	}
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum EdgeCondition {
	Fixed { state: bool },
	Direct { node: Id },
	Router,
}

impl EdgeCondition {
	fn convert(
		self,
		node_ids: &HashMap<Id, usize>,
		router: Option<(usize, Vec<(usize, usize)>)>,
	) -> lib::EdgeCondition {
		match self {
			Self::Fixed { state } => lib::EdgeCondition::Fixed { state },
			Self::Direct { node } => lib::EdgeCondition::Direct {
				node: *node_ids.get(&node).unwrap(),
			},
			Self::Router => {
				if let Some((block, routes)) = router {
					lib::EdgeCondition::Router { block, routes }
				} else {
					eprintln!("warning: edge is set to router but is not a block member");
					lib::EdgeCondition::Fixed { state: false }
				}
			},
		}
	}
}

impl Default for EdgeCondition {
	fn default() -> Self {
		Self::Fixed { state: false }
	}
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct BlockCondition {
	timer: ResetCondition,
}

impl BlockCondition {
	fn convert(self) -> lib::BlockCondition {
		lib::BlockCondition {
			reset: self
				.timer
				.map(|t| lib::ResetCondition::TimeSecs(t))
				.unwrap_or(lib::ResetCondition::None),
		}
	}
}

impl Default for BlockCondition {
	fn default() -> Self {
		Self { timer: None }
	}
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
