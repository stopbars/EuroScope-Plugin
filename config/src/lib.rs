use std::fmt::Debug;
use std::io::{ Read, Write };

use bincode::{ DefaultOptions, ErrorKind, Options };
pub use bincode;

use serde::{ Deserialize, Serialize };

static MAGIC: &[u8] = b"\xffBARS\x13eu";
const VERSION: u16 = 0;

fn bincode_options() -> impl Options {
	DefaultOptions::new().with_limit(0x100_0000)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
	pub name: Option<String>,
	pub version: Option<String>,

	pub aerodromes: Vec<Aerodrome>,
}

impl Config {
	pub fn load(mut reader: impl Read) -> bincode::Result<Self> {
		let mut buf = vec![0; MAGIC.len()];
		reader.read_exact(&mut buf)?;

		if buf != MAGIC {
			return Err(ErrorKind::Custom("invalid config file".into()).into())
		}

		let mut buf = [0; 2];
		reader.read_exact(&mut buf)?;

		if buf != VERSION.to_be_bytes() {
			return Err(ErrorKind::Custom("unsupported config version".into()).into())
		}

		bincode_options().deserialize_from(reader)
	}

	pub fn save(&self, mut writer: impl Write) -> bincode::Result<()> {
		writer.write_all(&MAGIC)?;
		writer.write_all(&VERSION.to_be_bytes())?;

		bincode_options().serialize_into(writer, self)
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Aerodrome {
	pub icao: String,

	pub elements: Vec<Element>,
	pub nodes: Vec<Node>,
	pub edges: Vec<Edge>,
	pub blocks: Vec<Block>,

	pub profiles: Vec<Profile>,

	pub maps: Vec<Map>,
	pub views: Vec<View>,
	pub styles: Vec<Style>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Element {
	pub id: String,
	pub condition: ElementCondition,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum ElementCondition {
	Fixed(bool),
	Node(usize),
	Edge(usize),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Node {
	pub id: String,

	pub scratchpad: Option<String>,
	pub parent: Option<usize>,

	pub display: NodeDisplay<GeoPoint>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Edge {
	pub display: EdgeDisplay<GeoPoint>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Block {
	pub id: String,

	pub nodes: Vec<usize>,
	pub edges: Vec<usize>,
	pub non_routes: Vec<(usize, usize)>,

	pub stands: Vec<String>,

	pub display: BlockDisplay<GeoPoint>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Profile {
	pub id: String,
	pub name: String,

	pub nodes: Vec<NodeCondition>,
	pub edges: Vec<EdgeCondition>,
	pub blocks: Vec<BlockCondition>,

	pub presets: Vec<Preset>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum NodeCondition {
	Fixed(bool),
	Direct(ResetCondition),
	Router,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum EdgeCondition {
	Fixed(bool),
	Direct(usize),
	Router {
		block: usize,
		routes: Vec<(usize, usize)>,
	},
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct BlockCondition(pub ResetCondition);

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum ResetCondition {
	None,
	TimeSecs(u32),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Preset {
	pub name: String,

	pub nodes: Vec<(usize, bool)>,
	pub blocks: Vec<(usize, BlockState)>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum BlockState {
	Clear,
	Relax,
	Route((usize, usize)),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Map {
	pub background: Color,
	pub base: Vec<Path<Point>>,

	pub nodes: Vec<NodeDisplay<Point>>,
	pub edges: Vec<EdgeDisplay<Point>>,
	pub blocks: Vec<BlockDisplay<Point>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct View {
	pub name: String,

	pub map: usize,
	pub bounds: Box,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Box {
	pub min: Point,
	pub max: Point,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Point {
	pub x: f32,
	pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Geo {
	pub lat: f32,
	pub lon: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct GeoPoint {
	pub geo: Geo,
	pub offset: Point,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Path<T: Clone + Debug> {
	pub points: Vec<T>,
	pub style: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Target<T: Clone + Debug> {
	pub points: Vec<T>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NodeDisplay<T: Clone + Debug> {
	pub off: Vec<Path<T>>,
	pub on: Vec<Path<T>>,
	pub selected: Vec<Path<T>>,

	pub target: Target<T>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EdgeDisplay<T: Clone + Debug> {
	pub off: Vec<Path<T>>,
	pub on: Vec<Path<T>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlockDisplay<T: Clone + Debug> {
	pub target: Target<T>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Style {
	pub stroke_width: f32,
	pub stroke_color: Color,

	pub fill_style: FillStyle,
	pub fill_color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Color {
	pub r: u8,
	pub g: u8,
	pub b: u8,
	pub a: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum FillStyle {
	None,
	Solid,
	HatchHorizontal,
	HatchVertical,
	HatchForwardDiagonal,
	HatchBackwardDiagonal,
	HatchCross,
	HatchDiagonalCross
}
