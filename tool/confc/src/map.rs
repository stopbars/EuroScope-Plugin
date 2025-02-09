use crate::Id;

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;

use bars_config::{
	BlockDisplay, Color, EdgeDisplay, FillStyle, Geo, GeoPoint, NodeDisplay,
	Path, Point, Style, Target,
};

use kml::types::{Geometry, Placemark, Style as KmlStyle, StyleMap};
use kml::{Kml as KmlItem, KmlDocument};

use kurbo::PathEl;

use usvg::tiny_skia_path::PathSegment;
use usvg::{Group, Node, Paint, Tree};

pub fn convert<T: Clone + Debug + MinMax>(
	input: impl Input<Point = T>,
	styles_offset: usize,
) -> Map<T> {
	#[derive(Clone, Copy, PartialEq)]
	enum Context {
		None,
		Basemap,
		Views,
		NodesOff,
		NodesOn,
		NodesSelected,
		NodesTarget,
		EdgesOff,
		EdgesOn,
		BlocksTarget,
	}

	fn visit<T: Clone + Debug + MinMax>(
		input: impl Input<Point = T>,
		map: &mut Map<T>,
		mut context: Context,
		mut id: Cow<str>,
		styles: &mut HashMap<TempStyle, usize>,
		styles_offset: usize,
	) {
		static SPLIT_CHARS: &[char] = &['_', ' ']; // inserted by Figma

		if let Some(group_id) = input.id() {
			context = match group_id {
				"basemap" => Context::Basemap,
				"views" => Context::Views,
				"nodes:off" => Context::NodesOff,
				"nodes:on" => Context::NodesOn,
				"nodes:selected" => Context::NodesSelected,
				"nodes:target" => Context::NodesTarget,
				"edges:off" => Context::EdgesOff,
				"edges:on" => Context::EdgesOn,
				"blocks:target" => Context::BlocksTarget,
				_ => {
					if let Some((_, group_id)) = group_id.split_once(':') {
						id = Cow::Owned(
							group_id
								.split_once(SPLIT_CHARS)
								.map(|s| s.0)
								.unwrap_or(&group_id)
								.into(),
						);
					}

					context
				},
			};
		}

		for input_path in input.paths() {
			let id = if let Some((_, id)) = input_path
				.id
				.as_ref()
				.map(|s| s.as_str())
				.unwrap_or("")
				.split_once(':')
			{
				id.split_once(SPLIT_CHARS).map(|s| s.0).unwrap_or(&id)
			} else {
				id.as_ref()
			};

			if id.len() > 0 && context == Context::Views {
				map.views.push((
					id.to_string(),
					(
						input_path
							.points
							.iter()
							.cloned()
							.reduce(|a, b| a.min(&b))
							.unwrap(),
						input_path
							.points
							.iter()
							.cloned()
							.reduce(|a, b| a.max(&b))
							.unwrap(),
					),
				));

				continue
			}

			let style = styles.entry(input_path.style).or_insert_with(|| {
				map.styles.push(Style {
					stroke_width: input_path.style.stroke_width as f32,
					stroke_color: input_path.style.stroke_color,
					fill_style: if input_path.style.fill.is_some() {
						FillStyle::Solid
					} else {
						FillStyle::None
					},
					fill_color: input_path.style.fill.unwrap_or_default(),
				});

				styles_offset + map.styles.len() - 1
			});
			let path = Path {
				points: input_path.points,
				style: *style,
			};

			if context == Context::Basemap {
				map.base.push(path);
				continue
			}

			if id.is_empty() || context == Context::None {
				continue
			}

			let id = Id(id.into());

			match context {
				Context::NodesOff
				| Context::NodesOn
				| Context::NodesSelected
				| Context::NodesTarget => {
					let ent = map.nodes.entry(id).or_insert_with(|| NodeDisplay {
						off: Vec::new(),
						on: Vec::new(),
						selected: Vec::new(),
						target: Target { points: Vec::new() },
					});

					match context {
						Context::NodesOff => ent.off.push(path),
						Context::NodesOn => ent.on.push(path),
						Context::NodesSelected => ent.selected.push(path),
						Context::NodesTarget => {
							ent.target = Target {
								points: path.points,
							}
						},
						_ => unreachable!(),
					}
				},
				Context::EdgesOff | Context::EdgesOn => {
					let ent = map.edges.entry(id).or_insert_with(|| EdgeDisplay {
						off: Vec::new(),
						on: Vec::new(),
					});

					match context {
						Context::EdgesOff => ent.off.push(path),
						Context::EdgesOn => ent.on.push(path),
						_ => unreachable!(),
					}
				},
				Context::BlocksTarget => {
					map.blocks.insert(
						id,
						BlockDisplay {
							target: Target {
								points: path.points,
							},
						},
					);
				},
				_ => unreachable!(),
			}
		}

		for group in input.groups() {
			visit(
				group,
				map,
				context,
				Cow::Borrowed(&id),
				styles,
				styles_offset,
			);
		}
	}

	let mut map = Map {
		base: Vec::new(),
		nodes: HashMap::new(),
		edges: HashMap::new(),
		blocks: HashMap::new(),
		views: Vec::new(),
		styles: Vec::new(),
	};
	let mut styles = HashMap::new();

	visit(
		input,
		&mut map,
		Context::None,
		Cow::Borrowed(""),
		&mut styles,
		styles_offset,
	);

	map
}

#[derive(Debug)]
pub struct Map<T: Clone + Debug> {
	pub base: Vec<Path<T>>,

	pub nodes: HashMap<Id, NodeDisplay<T>>,
	pub edges: HashMap<Id, EdgeDisplay<T>>,
	pub blocks: HashMap<Id, BlockDisplay<T>>,

	pub views: Vec<(String, (T, T))>,

	pub styles: Vec<Style>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct TempStyle {
	stroke_width: u8,
	stroke_color: Color,

	fill: Option<Color>,
}

pub struct TempPath<T> {
	id: Option<String>,
	points: Vec<T>,
	style: TempStyle,
}

/* impl<T> TempPath<T> {
	pub fn debug(&self) -> String {
		let ls = if self.style.stroke_width > 0 {
			let Color { r, g, b, .. } = self.style.stroke_color;
			format!("\x1b[38;2;{r};{g};{b}m──\x1b[0m")
		} else {
			"  ".into()
		};

		if let Some(Color { r, g, b, .. }) = self.style.fill {
			format!("\x1b[48;2;{r};{g};{b}m{ls}\x1b[0m {} pts", self.points.len())
		} else {
			format!("{ls} {} pts", self.points.len())
		}
	}
} */

pub trait MinMax {
	fn min(&self, other: &Self) -> Self;
	fn max(&self, other: &Self) -> Self;
}

impl MinMax for Point {
	fn min(&self, other: &Self) -> Self {
		Self {
			x: self.x.min(other.x),
			y: self.y.min(other.y),
		}
	}

	fn max(&self, other: &Self) -> Self {
		Self {
			x: self.x.max(other.x),
			y: self.y.max(other.y),
		}
	}
}

// fake impl, views are not used for geo displays
impl MinMax for GeoPoint {
	fn min(&self, _other: &Self) -> Self {
		*self
	}

	fn max(&self, _other: &Self) -> Self {
		*self
	}
}

pub trait Input: Sized {
	type Point;

	fn id(&self) -> Option<&str>;
	fn groups(&self) -> Vec<Self>;
	fn paths(&self) -> impl Iterator<Item = TempPath<Self::Point>>;
}

pub struct Svg<'a> {
	group: &'a Group,
}

impl<'a> Svg<'a> {
	pub fn new(svg: &'a Tree) -> Self {
		Self { group: svg.root() }
	}
}

const FLATTENING_TOLERANCE: f64 = 0.5;

impl Input for Svg<'_> {
	type Point = Point;

	fn id(&self) -> Option<&str> {
		match self.group.id() {
			"" => None,
			s => Some(s),
		}
	}

	fn groups(&self) -> Vec<Self> {
		self
			.group
			.children()
			.iter()
			.filter_map(|node| match node {
				Node::Group(group) => Some(Self {
					group: group.as_ref(),
				}),
				_ => None,
			})
			.collect()
	}

	fn paths(&self) -> impl Iterator<Item = TempPath<Self::Point>> {
		self.group.children().iter().filter_map(|node| {
			if let Node::Path(path) = node {
				let mut style = TempStyle {
					stroke_width: 0,
					stroke_color: Color::default(),
					fill: path.fill().map(|fill| {
						let Paint::Color(color) = fill.paint() else {
							unimplemented!()
						};
						Color {
							r: color.red,
							g: color.blue,
							b: color.blue,
							a: fill.opacity().to_u8(),
						}
					}),
				};

				if let Some(stroke) = path.stroke() {
					style.stroke_width = stroke.width().get().ceil() as u8;

					let Paint::Color(color) = stroke.paint() else {
						unimplemented!()
					};
					style.stroke_color = Color {
						r: color.red,
						g: color.blue,
						b: color.blue,
						a: stroke.opacity().to_u8(),
					};
				}

				let mut data = path.data().segments();
				data.set_auto_close(true);

				let mut points = Vec::new();

				fn c(point: usvg::tiny_skia_path::Point) -> kurbo::Point {
					kurbo::Point {
						x: point.x as f64,
						y: point.y as f64,
					}
				}

				kurbo::flatten(
					data.into_iter().map(|segment| match segment {
						PathSegment::MoveTo(p) => PathEl::MoveTo(c(p)),
						PathSegment::LineTo(p) => PathEl::LineTo(c(p)),
						PathSegment::QuadTo(p, q) => PathEl::QuadTo(c(p), c(q)),
						PathSegment::CubicTo(p, q, r) => PathEl::CurveTo(c(p), c(q), c(r)),
						PathSegment::Close => PathEl::ClosePath,
					}),
					FLATTENING_TOLERANCE,
					|el| {
						let p = match el {
							PathEl::MoveTo(p) => p,
							PathEl::LineTo(p) => p,
							PathEl::ClosePath => return,
							_ => unreachable!(),
						};
						points.push(Point {
							x: p.x as f32,
							y: p.y as f32,
						});
					},
				);

				Some(TempPath {
					id: match path.id() {
						"" => None,
						s => Some(s.into()),
					},
					points,
					style,
				})
			} else {
				None
			}
		})
	}
}

pub struct Kml {
	document: KmlDocument<f32>,
	styles: Rc<RefCell<HashMap<String, TempStyle>>>, // cba
}

impl Kml {
	pub fn new(kml: KmlItem<f32>) -> Option<Self> {
		if let KmlItem::KmlDocument(document) = kml {
			Some(Self {
				document,
				styles: Rc::new(RefCell::new(HashMap::new())),
			})
		} else {
			None
		}
	}

	pub fn input(&mut self) -> KmlInput<'_> {
		KmlInput::new(&self.document.elements, self.styles.clone())
	}
}

#[derive(Clone)]
pub struct KmlInput<'a> {
	children: &'a Vec<KmlItem<f32>>,
	styles: Rc<RefCell<HashMap<String, TempStyle>>>,
}

impl<'a> KmlInput<'a> {
	fn new(
		children: &'a Vec<KmlItem<f32>>,
		styles: Rc<RefCell<HashMap<String, TempStyle>>>,
	) -> Self {
		fn parse_color(s: &str) -> Option<Color> {
			u32::from_str_radix(s, 16)
				.ok()
				.map(|abgr| abgr.to_be_bytes())
				.map(|[a, b, g, r]| Color { r, g, b, a })
		}

		{
			let mut styles_ref = styles.borrow_mut();

			for child in children {
				if let KmlItem::Style(KmlStyle {
					id: Some(id),
					line,
					poly,
					..
				}) = child
				{
					let mut style = TempStyle {
						stroke_width: line
							.as_ref()
							.map(|s| s.width.ceil() as u8)
							.unwrap_or(0),
						stroke_color: line
							.as_ref()
							.and_then(|s| parse_color(&s.color))
							.unwrap_or_default(),
						fill: poly.as_ref().and_then(|s| parse_color(&s.color)),
					};

					if style.fill.is_none() && style.stroke_width == 0 {
						style.stroke_width = 1;
						style.stroke_color = parse_color("ffffffff").unwrap();
					}

					styles_ref.insert(format!("#{id}"), style);
				}
			}

			for child in children {
				if let KmlItem::StyleMap(StyleMap {
					id: Some(id),
					pairs,
					..
				}) = child
				{
					for pair in pairs {
						let style = *styles_ref.get(&pair.style_url).unwrap();
						styles_ref.insert(format!("#{}", id), style);
					}
				}
			}
		}

		Self { children, styles }
	}
}

impl Input for KmlInput<'_> {
	type Point = GeoPoint;

	fn id(&self) -> Option<&str> {
		self.children.iter().find_map(|kml| {
			if let KmlItem::Element(element) = kml {
				(element.name == "name")
					.then_some(element.content.as_ref().map(|s| s.as_str()))
					.flatten()
			} else {
				None
			}
		})
	}

	fn groups(&self) -> Vec<Self> {
		self
			.children
			.iter()
			.filter_map(|kml| match kml {
				KmlItem::KmlDocument(KmlDocument { elements, .. })
				| KmlItem::Folder { elements, .. }
				| KmlItem::Document { elements, .. } => {
					Some(Self::new(elements, self.styles.clone()))
				},
				_ => None,
			})
			.collect()
	}

	fn paths(&self) -> impl Iterator<Item = TempPath<<Self as Input>::Point>> {
		fn convert_geometry(
			geom: &Geometry<f32>,
			id: &Option<String>,
			style: TempStyle,
		) -> Vec<TempPath<GeoPoint>> {
			let coords = match geom {
				Geometry::LineString(line) => &line.coords,
				Geometry::LinearRing(ring) => &ring.coords,
				Geometry::Polygon(poly) => &poly.outer.coords,
				Geometry::MultiGeometry(multi) => {
					let mut vec = Vec::new();
					for geom in &multi.geometries {
						vec.append(&mut convert_geometry(geom, id, style));
					}
					return vec
				},
				_ => return Vec::new(),
			};

			if coords.is_empty() {
				return Vec::new()
			}

			let points = coords
				.into_iter()
				.map(|point| GeoPoint {
					geo: Geo {
						lat: point.y,
						lon: point.x,
					},
					offset: Point::default(),
				})
				.collect::<Vec<_>>();

			vec![TempPath {
				id: id.clone(),
				points,
				style,
			}]
		}

		self
			.children
			.iter()
			.filter_map(|kml| {
				if let KmlItem::Placemark(Placemark {
					name,
					geometry: Some(geom),
					style_url: Some(style_url),
					..
				}) = kml
				{
					let styles = self.styles.borrow();
					let style = styles.get(style_url)?;

					Some(convert_geometry(geom, &name, *style))
				} else {
					None
				}
			})
			.flatten()
	}
}

pub struct GeoSvg<'a> {
	svg: Svg<'a>,
	transform: [f64; 4],
}

impl<'a> GeoSvg<'a> {
	pub fn new(svg: &'a Tree, lat: (f64, f64), lon: (f64, f64)) -> Self {
		let size = svg.size();

		Self {
			svg: Svg::new(svg),
			transform: [
				(lat.1 - lat.0) / size.height() as f64,
				lat.0,
				(lon.1 - lon.0) / size.width() as f64,
				lon.0,
			],
		}
	}

	fn transform(&self, p: Point) -> GeoPoint {
		GeoPoint {
			geo: Geo {
				lat: (self.transform[0] * p.y as f64 + self.transform[1]) as f32,
				lon: (self.transform[2] * p.x as f64 + self.transform[3]) as f32,
			},
			offset: Point::default(),
		}
	}
}

impl Input for GeoSvg<'_> {
	type Point = GeoPoint;

	fn id(&self) -> Option<&str> {
		self.svg.id()
	}

	fn groups(&self) -> Vec<Self> {
		self
			.svg
			.group
			.children()
			.iter()
			.filter_map(|node| match node {
				Node::Group(group) => Some(Self {
					svg: Svg {
						group: group.as_ref(),
					},
					transform: self.transform,
				}),
				_ => None,
			})
			.collect()
	}

	fn paths(&self) -> impl Iterator<Item = TempPath<<Self as Input>::Point>> {
		self.svg.paths().map(|path| TempPath {
			id: path.id,
			points: path
				.points
				.into_iter()
				.map(|point| self.transform(point))
				.collect(),
			style: path.style,
		})
	}
}
