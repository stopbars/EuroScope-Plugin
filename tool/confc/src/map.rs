use crate::Id;

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::rc::Rc;

use bars_config::{
	BlockDisplay, Color, EdgeDisplay, Geo, GeoPoint, NodeDisplay, Path, Point,
};

use kml::{ Kml as KmlItem, KmlDocument };
use kml::types::{ Geometry, Placemark, Style as KmlStyle, StyleMap };

use usvg::{ Group, Node, Paint, Tree };
use usvg::tiny_skia_path::PathSegment;

#[derive(Debug)]
pub struct Map<T: Clone + Debug> {
	pub base: Vec<Path<T>>,

	pub nodes: HashMap<Id, NodeDisplay<T>>,
	pub edges: HashMap<Id, EdgeDisplay<T>>,
	pub blocks: HashMap<Id, BlockDisplay<T>>,

	pub views: Vec<(String, (T, T))>,
}

#[derive(Clone, Copy, Debug, Hash)]
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
		Self {
			group: svg.root(),
		}
	}
}

impl Input for Svg<'_> {
	type Point = Point;

	fn id(&self) -> Option<&str> {
		match self.group.id() {
			"" => None,
			s  => Some(s),
		}
	}

	fn groups(&self) -> Vec<Self> {
		self.group
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
		self.group
			.children()
			.iter()
			.filter_map(|node| if let Node::Path(path) = node {
				let mut style = TempStyle {
					stroke_width: 0,
					stroke_color: Color::default(),
					fill: path.fill().map(|fill| {
						let Paint::Color(color) = fill.paint() else { unimplemented!() };
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

					let Paint::Color(color) = stroke.paint() else { unimplemented!() };
					style.stroke_color = Color {
						r: color.red,
						g: color.blue,
						b: color.blue,
						a: stroke.opacity().to_u8(),
					};
				}

				let mut data = path.data().segments();
				data.set_auto_close(true);

				Some(TempPath {
					id: match path.id() {
						"" => None,
						s  => Some(s.into()),
					},
					points: data
						.filter_map(|segment| match segment {
							PathSegment::MoveTo(p) => Some(p),
							PathSegment::LineTo(p) => Some(p),
							PathSegment::Close     => None,
							_ => unimplemented!(),
						})
						.map(|point| Point { x: point.x, y: point.y })
						.collect(),
					style,
				})
			} else {
				None
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
				}) = child {
					let mut style = TempStyle {
						stroke_width: line
							.as_ref()
							.map(|s| s.width.ceil() as u8)
							.unwrap_or(0),
						stroke_color: line
							.as_ref()
							.and_then(|s| parse_color(&s.color))
							.unwrap_or_default(),
						fill: poly
							.as_ref()
							.and_then(|s| parse_color(&s.color)),
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
				}) = child {
					for pair in pairs {
						let style = *styles_ref.get(&pair.style_url).unwrap();
						styles_ref.insert(format!("#{}", id), style);
					}
				}
			}
		}

		Self {
			children,
			styles,
		}
	}
}

impl Input for KmlInput<'_> {
	type Point = GeoPoint;

	fn id(&self) -> Option<&str> {
		self.children
			.iter()
			.find_map(|kml| if let KmlItem::Element(element) = kml {
				(element.name == "name")
					.then_some(element.content.as_ref().map(|s| s.as_str()))
					.flatten()
			} else {
				None
			})
	}

	fn groups(&self) -> Vec<Self> {
		self.children
			.iter()
			.filter_map(|kml| match kml {
				KmlItem::KmlDocument(KmlDocument { elements, .. })
					| KmlItem::Folder { elements, .. }
					| KmlItem::Document { elements, .. }
				=> Some(Self::new(elements, self.styles.clone())),
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

		self.children
			.iter()
			.filter_map(|kml| if let KmlItem::Placemark(Placemark {
				name,
				geometry: Some(geom),
				style_url: Some(style_url),
				..
			}) = kml {
				let styles = self.styles.borrow();
				let style = styles.get(style_url)?;

				Some(convert_geometry(geom, &name, *style))
			} else {
				None
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
				(lat.1 - lat.0) / size.height() as f64, lat.0,
				(lon.1 - lon.0) / size.width()  as f64, lon.0,
			]
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
		self.svg.group
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
		self.svg
			.paths()
			.map(|path| TempPath {
				id: path.id,
				points: path.points
					.into_iter()
					.map(|point| self.transform(point))
					.collect(),
				style: path.style,
			})
	}
}
