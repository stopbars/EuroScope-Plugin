use crate::client::Aerodrome;
use crate::context::Context;
use crate::{ActivityState, ClickType, ViewportGeo, ViewportNonGeo};

use std::fmt::Debug;
use std::time::{Duration, Instant};

use bars_config::{
	BlockDisplay, BlockState, Color, EdgeCondition, EdgeDisplay, FillStyle, Geo,
	GeoPoint, NodeCondition, NodeDisplay, Path, Point,
};

use tracing::{trace, warn};

use windows::Win32::Foundation::{COLORREF, POINT, RECT};
use windows::Win32::Graphics::Gdi::{self, HBRUSH, HDC, HPEN};

const DESELECT_AFTER: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Default)]
enum Target {
	#[default]
	None,
	Node(u16),
	Block(u16),
}

struct Style {
	brush: HBRUSH,
	pen: HPEN,
	filled: bool,
}

impl Style {
	unsafe fn new(style: &bars_config::Style) -> Self {
		fn color(color: Color) -> COLORREF {
			COLORREF(
				((color.b as u32) << 16) | ((color.g as u32) << 8) | color.r as u32,
			)
		}

		let brush = if style.fill_style == FillStyle::None {
			HBRUSH(Gdi::GetStockObject(Gdi::NULL_BRUSH).0)
		} else if style.fill_style == FillStyle::Solid {
			Gdi::CreateSolidBrush(color(style.fill_color))
		} else {
			Gdi::CreateHatchBrush(
				match style.fill_style {
					FillStyle::None | FillStyle::Solid => unreachable!(),
					FillStyle::HatchHorizontal => Gdi::HS_HORIZONTAL,
					FillStyle::HatchVertical => Gdi::HS_VERTICAL,
					FillStyle::HatchForwardDiagonal => Gdi::HS_FDIAGONAL,
					FillStyle::HatchBackwardDiagonal => Gdi::HS_BDIAGONAL,
					FillStyle::HatchCross => Gdi::HS_CROSS,
					FillStyle::HatchDiagonalCross => Gdi::HS_DIAGCROSS,
				},
				color(style.fill_color),
			)
		};

		let pen = if style.stroke_width > 0.0 {
			Gdi::CreatePen(
				Gdi::PS_SOLID,
				style.stroke_width.ceil() as i32,
				color(style.stroke_color),
			)
		} else {
			HPEN(Gdi::GetStockObject(Gdi::NULL_PEN).0)
		};

		Self {
			brush,
			pen,
			filled: style.fill_style != FillStyle::None,
		}
	}

	unsafe fn apply(&self, hdc: HDC) {
		Gdi::SelectObject(hdc, self.brush.into());
		Gdi::SelectObject(hdc, self.pen.into());
	}
}

impl Drop for Style {
	fn drop(&mut self) {
		unsafe {
			let _ = Gdi::DeleteObject(self.brush.into());
			let _ = Gdi::DeleteObject(self.pen.into());
		}
	}
}

pub struct Screen<'a> {
	context: &'a mut Context,
	icao: Option<String>,
	view: Option<usize>,
	transform: Transform,
	targets: Option<Lookup2d<Target>>,
	click_regions: Vec<RECT>,
	selected: Option<(usize, Instant)>,
	styles: Vec<Style>,
	refresh_required: bool,
	last_controlling: bool,
	last_data: bool,
	last_profile: usize,
}

impl<'a> Screen<'a> {
	pub fn new(context: &'a mut Context, geo: bool) -> Self {
		Self {
			context,
			icao: None,
			view: (!geo).then_some(0),
			transform: Transform::new(),
			targets: None,
			click_regions: Vec::new(),
			selected: None,
			styles: Vec::new(),
			refresh_required: true,
			last_controlling: false,
			last_data: false,
			last_profile: usize::MAX,
		}
	}
}

impl Screen<'_> {
	fn data(&self) -> Option<&Aerodrome> {
		self.icao.as_ref().and_then(|icao| {
			self
				.context
				.client()
				.and_then(|client| client.aerodrome(icao))
		})
	}

	fn data_mut(&mut self) -> Option<&mut Aerodrome> {
		self.icao.as_ref().and_then(|icao| {
			self
				.context
				.client_mut()
				.and_then(|client| client.aerodrome_mut(icao))
		})
	}

	pub fn aerodrome(&self) -> Option<&str> {
		self.icao.as_ref().map(|s| s.as_str())
	}

	pub fn set_aerodrome(&mut self, icao: Option<&str>) {
		if let Some(icao) = &self.icao {
			self.context.untrack_aerodrome(icao);
		}
		if let Some(icao) = &icao {
			self.context.track_aerodrome(icao.to_string());
		}

		self.icao = icao.map(|s| s.to_string());

		if let Some(targets) = self.targets.as_mut() {
			targets.clear(Target::None);
		}
		self.styles.clear();

		self.refresh_required = true;
		self.last_controlling = false;
		self.last_profile = usize::MAX;
	}

	pub fn state(&self) -> ActivityState {
		self
			.data()
			.map(|aerodrome| aerodrome.state())
			.unwrap_or(ActivityState::None)
	}

	pub fn set_state(&mut self, state: ActivityState) {
		if state == ActivityState::None {
			return
		}

		if let Some((c, icao)) = self.context.client_mut().zip(self.icao.as_ref()) {
			if let Err(err) =
				c.set_controlling(icao.clone(), state == ActivityState::Controlling)
			{
				warn!("failed to set state: {err}");
			}

			self.refresh_required = true;
		}
	}

	pub fn profiles(&self) -> Vec<String> {
		self
			.data()
			.map(|aerodrome| {
				aerodrome
					.config()
					.profiles
					.iter()
					.map(|profile| profile.name.clone())
					.collect()
			})
			.unwrap_or(Vec::new())
	}

	pub fn profile(&self) -> usize {
		self
			.data()
			.map(|aerodrome| aerodrome.profile())
			.unwrap_or(0)
	}

	pub fn set_profile(&mut self, i: usize) {
		self.data_mut().map(|aerodrome| aerodrome.set_profile(i));
		self.refresh_required = true;
	}

	pub fn presets(&self) -> Vec<String> {
		self
			.data()
			.map(|aerodrome| {
				aerodrome.config().profiles[aerodrome.profile()]
					.presets
					.iter()
					.map(|preset| preset.name.clone())
					.collect()
			})
			.unwrap_or(Vec::new())
	}

	// bug: if profile changes between preset() and apply_preset(...), wrong
	// preset will be applied
	pub fn apply_preset(&mut self, i: usize) {
		self.data_mut().map(|aerodrome| aerodrome.apply_preset(i));
	}

	pub fn views(&self) -> Vec<String> {
		self
			.data()
			.map(|aerodrome| {
				aerodrome
					.config()
					.views
					.iter()
					.map(|view| view.name.clone())
					.collect()
			})
			.unwrap_or(Vec::new())
	}

	pub fn view(&self) -> usize {
		self.view.unwrap_or(0)
	}

	pub fn set_view(&mut self, i: usize) {
		if let Some(view) = self.view.as_mut() {
			*view = i;
			self.refresh_required = true;
		}
	}

	pub fn is_pilot_enabled(&self, callsign: &str) -> bool {
		self
			.data()
			.map(|aerodrome| aerodrome.is_pilot_enabled(callsign))
			.unwrap_or(false)
	}

	fn load_styles(&mut self) {
		self.styles = if let Some(data) = self.data() {
			data
				.config()
				.styles
				.iter()
				.map(|style| unsafe { Style::new(style) })
				.collect()
		} else {
			return
		};
	}

	fn project_points<T: Transformable>(&self, points: &[T]) -> Vec<(f64, f64)> {
		points
			.iter()
			.map(|p| p.transform(&self.transform))
			.collect()
	}

	unsafe fn draw_path<T: Clone + Debug + Transformable>(
		&self,
		hdc: HDC,
		path: &Path<T>,
	) {
		if path.style >= self.styles.len() {
			return
		}

		let style = &self.styles[path.style];
		style.apply(hdc);

		let points = path
			.points
			.iter()
			.map(|p| p.transform(&self.transform))
			.map(|(x, y)| POINT {
				x: x.round() as i32,
				y: y.round() as i32,
			})
			.collect::<Vec<_>>();

		if style.filled {
			let _ = Gdi::Polygon(hdc, points.as_slice());
		} else {
			let _ = Gdi::Polyline(hdc, points.as_slice());
		}
	}

	fn setup_targets<'a, T: Clone + Debug + Transformable + 'a>(
		&self,
		size: [f64; 2],
		nodes: impl Iterator<Item = &'a NodeDisplay<T>>,
		blocks: impl Iterator<Item = &'a BlockDisplay<T>>,
		targets: &mut Lookup2d<Target>,
	) {
		let width = size[0].round() as usize;
		let height = size[1].round() as usize;

		if targets.width == width && targets.data.len() == width * height {
			targets.clear(Target::None);
		} else {
			*targets = Lookup2d::new(Target::None, width, height);
		}

		for (i, block) in blocks.enumerate() {
			let points = self.project_points(&block.target.points);
			targets.add_poly(Target::Block(i as u16), &points);
		}

		let Some(aerodrome) = self.data() else { return };
		let profile = &aerodrome.config().profiles[aerodrome.profile()];

		for (i, node) in nodes.enumerate() {
			if !matches!(profile.nodes[i], NodeCondition::Fixed { .. }) {
				let points = self.project_points(&node.target.points);
				targets.add_poly(Target::Node(i as u16), &points);
			}
		}
	}

	fn is_controlling(&self) -> bool {
		self
			.data()
			.map(|aerodrome| aerodrome.state() == ActivityState::Controlling)
			.unwrap_or_default()
	}

	pub fn draw_background_geo(&mut self, _hdc: HDC, viewport: ViewportGeo) {
		const CELL_SIZE: usize = 20;
		const THRESHOLD: usize = 100;

		let instant_start = std::time::Instant::now();

		let _ = self.is_background_refresh_required();

		if self.styles.is_empty() {
			self.load_styles();
		}

		self.click_regions.clear();
		self.transform = Transform::new_geo(viewport);

		if !self.is_controlling() {
			return
		}

		let mut targets = self.targets.take().unwrap_or_default();

		let Some(aerodrome) = self.data() else { return };

		self.setup_targets(
			viewport.size,
			aerodrome.config().nodes.iter().map(|node| &node.display),
			aerodrome.config().blocks.iter().map(|block| &block.display),
			&mut targets,
		);

		// this isn't very good

		let width = viewport.size[0].round() as usize;
		let height = viewport.size[1].round() as usize;

		for by in 0..height / CELL_SIZE {
			let cy = by * CELL_SIZE;

			let mut startx = 0;

			for bx in 0..width / CELL_SIZE {
				let cx = bx * CELL_SIZE;

				let mut n = 0;
				'a: for x in 0..CELL_SIZE {
					for y in 0..CELL_SIZE {
						if !matches!(targets.sample(cx + x, cy + y), Target::None) {
							n += 1;
							if n > THRESHOLD {
								break 'a
							}
						}
					}
				}

				if n <= THRESHOLD {
					if startx < bx {
						self.click_regions.push(RECT {
							left: (startx * CELL_SIZE) as i32,
							top: cy as i32,
							right: cx as i32,
							bottom: (cy + CELL_SIZE) as i32,
						});
					}

					startx = bx + 1;
				}
			}

			if startx < width / CELL_SIZE {
				self.click_regions.push(RECT {
					left: (startx * CELL_SIZE) as i32,
					top: cy as i32,
					right: width as i32,
					bottom: (cy + CELL_SIZE) as i32,
				});
			}
		}

		self.targets = Some(targets);

		trace!("bg {:?}", instant_start.elapsed());
	}

	pub fn draw_background_non_geo(
		&mut self,
		hdc: HDC,
		viewport: ViewportNonGeo,
	) {
		let instant_start = std::time::Instant::now();

		let _ = self.is_background_refresh_required();

		if self.styles.is_empty() {
			self.load_styles();
		}

		self.click_regions.clear();

		if self.is_controlling() {
			self.click_regions.push(RECT {
				left: 0 as i32,
				top: 0 as i32,
				right: viewport.size[0] as i32,
				bottom: viewport.size[1] as i32,
			});
		}

		let mut targets = self.targets.take().unwrap_or_default();

		let Some(aerodrome) = self.data() else { return };
		let Some(view) = aerodrome.config().views.get(self.view.unwrap()) else {
			return
		};

		self.setup_targets(
			viewport.size,
			aerodrome.config().maps[view.map]
				.nodes
				.iter()
				.map(|node| node),
			aerodrome.config().maps[view.map]
				.blocks
				.iter()
				.map(|block| block),
			&mut targets,
		);

		self.transform = Transform::new_view(viewport, view.bounds);
		self.targets = Some(targets);

		let Some(aerodrome) = self.data() else { return };
		let Some(view) = aerodrome.config().views.get(self.view.unwrap()) else {
			return
		};

		let map = &aerodrome.config().maps[view.map];

		unsafe {
			Style::new(&bars_config::Style {
				stroke_width: 0.0,
				stroke_color: Color::default(),
				fill_style: FillStyle::Solid,
				fill_color: map.background,
			})
			.apply(hdc);
			let _ = Gdi::Rectangle(
				hdc,
				viewport.origin[0] as i32,
				viewport.origin[1] as i32,
				viewport.size[0] as i32,
				viewport.size[1] as i32,
			);
		}

		for path in &map.base {
			unsafe {
				self.draw_path(hdc, path);
			}
		}

		trace!("bg {:?}", instant_start.elapsed());
	}

	fn draw_items<'a, T: Clone + Debug + Transformable + 'a>(
		&self,
		aerodrome: &Aerodrome,
		nodes: impl Iterator<Item = &'a NodeDisplay<T>>,
		edges: impl Iterator<Item = &'a EdgeDisplay<T>>,
		hdc: HDC,
	) {
		for (i, edge) in edges.enumerate() {
			if let EdgeCondition::Fixed { state: false } =
				aerodrome.config().profiles[self.profile()].edges[i]
			{
				continue
			}

			let display = if aerodrome.edge_state(i) {
				&edge.on
			} else {
				&edge.off
			};

			for path in display {
				unsafe {
					self.draw_path(hdc, path);
				}
			}
		}

		for (i, node) in nodes.enumerate() {
			if aerodrome.config().nodes[i].parent.is_some() {
				continue
			}

			if aerodrome.config().profiles[self.profile()].nodes[i]
				== (NodeCondition::Fixed { state: false })
			{
				continue
			}

			let display = if aerodrome.node_state(i) {
				&node.on
			} else {
				&node.off
			};

			for path in display {
				unsafe {
					self.draw_path(hdc, path);
				}
			}

			if self.selected.map(|(n, _)| n == i).unwrap_or_default()
				&& self.selected.unwrap().1.elapsed() < DESELECT_AFTER
			{
				for path in &node.selected {
					unsafe {
						self.draw_path(hdc, path);
					}
				}
			}
		}
	}

	pub fn draw_foreground(&mut self, hdc: HDC) {
		let instant_start = std::time::Instant::now();

		let Some(aerodrome) = self.data() else { return };

		if let Some(view) = self.view {
			let map = &aerodrome.config().maps[aerodrome.config().views[view].map];

			self.draw_items(aerodrome, map.nodes.iter(), map.edges.iter(), hdc);
		} else {
			self.draw_items(
				aerodrome,
				aerodrome.config().nodes.iter().map(|node| &node.display),
				aerodrome.config().edges.iter().map(|edge| &edge.display),
				hdc,
			);
		}

		if instant_start.elapsed() > Duration::from_millis(1) {
			trace!("fg {:?}", instant_start.elapsed());
		}
	}

	pub fn set_viewport_geo(&mut self, viewport: ViewportGeo) {
		self.transform = Transform::new_geo(viewport);
	}

	pub fn set_viewport_non_geo(&mut self, viewport: ViewportNonGeo) {
		let Some(aerodrome) = self.data() else { return };
		let Some(view) = self.view else { return };

		let bounds = aerodrome.config().views[view].bounds;
		self.transform = Transform::new_view(viewport, bounds);
	}

	pub fn click_regions(&self) -> &[RECT] {
		&self.click_regions
	}

	pub fn handle_click(
		&mut self,
		point: POINT,
		click: ClickType,
	) -> Option<String> {
		let target = self
			.targets
			.as_ref()
			.map(|targets| *targets.sample(point.x as usize, point.y as usize))
			.unwrap_or(Target::None);

		let selection = self.selected.take();
		let geo = self.view.is_none();

		let Some(data) = self.data_mut() else {
			return None
		};

		match target {
			Target::None => {
				if geo {
					self.selected = selection;
				}

				None
			},
			Target::Node(id) => {
				if click == ClickType::Primary {
					match data.config().profiles[data.profile()].nodes[id as usize] {
						NodeCondition::Fixed { .. } => (),
						NodeCondition::Direct { .. } => {
							data.set_node(id as usize, !data.node_state(id as usize));
						},
						NodeCondition::Router => {
							if let Some((node, at)) = selection {
								if at.elapsed() < DESELECT_AFTER {
									data.set_route((node, id as usize));
								}
							}

							self.selected = Some((id as usize, Instant::now()));
						},
					}

					None
				} else {
					data.config().nodes[id as usize].scratchpad.clone()
				}
			},
			Target::Block(id) => {
				data.set_block(
					id as usize,
					match click {
						ClickType::Primary => BlockState::Clear,
						ClickType::Auxiliary => BlockState::Relax,
					},
				);

				None
			},
		}
	}

	#[must_use]
	pub fn is_background_refresh_required(&mut self) -> bool {
		let controlling = self.is_controlling();
		let data = self.data().is_some();
		let profile = self.profile();

		let explicit = std::mem::take(&mut self.refresh_required);
		let controlling =
			std::mem::replace(&mut self.last_controlling, controlling);
		let data = std::mem::replace(&mut self.last_data, data);
		let profile = std::mem::replace(&mut self.last_profile, profile);

		explicit
			|| controlling != self.last_controlling
			|| data != self.last_data
			|| profile != self.last_profile
	}
}

impl Drop for Screen<'_> {
	fn drop(&mut self) {
		if let Some(icao) = &self.icao {
			self.context.untrack_aerodrome(icao);
		}
	}
}

#[derive(Debug, Default)]
struct Transform(f64, f64, f64, f64, f64, f64);

impl Transform {
	fn new() -> Self {
		Self::default()
	}

	fn new_geo(viewport: ViewportGeo) -> Self {
		let sin = viewport.rotation.sin();
		let cos = viewport.rotation.cos();

		let klat = -viewport.scaling[0] * viewport.origin[0];
		let klon = -viewport.scaling[1] * viewport.origin[1];

		Self(
			viewport.scaling[0] * cos,
			viewport.scaling[1] * sin,
			klon * sin + klat * cos,
			viewport.scaling[0] * -sin,
			viewport.scaling[1] * cos,
			klon * cos - klat * sin,
		)
	}

	fn new_view(viewport: ViewportNonGeo, bounds: bars_config::Box) -> Self {
		let bounds_w = (bounds.max.x - bounds.min.x) as f64;
		let bounds_h = (bounds.max.y - bounds.min.y) as f64;

		let viewport_ratio = viewport.size[0] / viewport.size[1];
		let bounds_ratio = bounds_w / bounds_h;

		let (scale, offset_x, offset_y) = if bounds_ratio > viewport_ratio {
			let scale = viewport.size[0] / bounds_w;
			(scale, 0.0, (viewport.size[1] - bounds_h * scale) * 0.5)
		} else {
			let scale = viewport.size[1] / bounds_h;
			(scale, (viewport.size[0] - bounds_w * scale) * 0.5, 0.0)
		};

		Self(
			scale,
			0.0,
			scale * -bounds.min.x as f64 + offset_x,
			0.0,
			scale,
			scale * -bounds.min.y as f64 + offset_y,
		)
	}

	fn transform(&self, (x, y): (f64, f64)) -> (f64, f64) {
		(
			x * self.0 + y * self.1 + self.2,
			x * self.3 + y * self.4 + self.5,
		)
	}

	fn transform_geo(&self, geo: &Geo) -> (f64, f64) {
		self.transform((geo.lat as f64, geo.lon as f64))
	}

	fn transform_geo_point(&self, gp: &GeoPoint) -> (f64, f64) {
		let (x, y) = self.transform_geo(&gp.geo);
		(x + gp.offset.x as f64, y + gp.offset.y as f64)
	}

	fn transform_point(&self, point: &Point) -> (f64, f64) {
		self.transform((point.x as f64, point.y as f64))
	}
}

trait Transformable {
	fn transform(&self, transform: &Transform) -> (f64, f64);
}

impl Transformable for Point {
	fn transform(&self, transform: &Transform) -> (f64, f64) {
		transform.transform_point(self)
	}
}

impl Transformable for GeoPoint {
	fn transform(&self, transform: &Transform) -> (f64, f64) {
		transform.transform_geo_point(self)
	}
}

#[derive(Default)]
struct Lookup2d<T> {
	data: Vec<T>,
	width: usize,
}

impl<T: Copy> Lookup2d<T> {
	fn new(item: T, width: usize, height: usize) -> Self {
		Self {
			data: vec![item; width * height],
			width,
		}
	}

	fn sample(&self, x: usize, y: usize) -> &T {
		&self.data[(x + y * self.width).min(self.data.len() - 1)]
	}

	fn clear(&mut self, item: T) {
		self.data.fill(item);
	}

	fn add_poly(&mut self, item: T, points: &[(f64, f64)]) {
		let (min, max) = points
			.iter()
			.map(|(_, y)| y.max(0.0).round() as usize)
			.fold((usize::MAX, 0), |(min, max), y| (min.min(y), max.max(y)));
		let max_y = self.data.len() / self.width - 1;

		let min = min.min(max_y);
		let max = max.min(max_y);

		let mut intersections = Vec::new();
		for y in min..=max {
			let yf = y as f64 + 0.5;

			for i in 0..points.len() {
				let (x1, y1) = points[i];
				let (x2, y2) = points[(i + 1) % points.len()];

				if (y1 > yf) != (y2 > yf) {
					intersections.push(x1 + (x2 - x1) * (yf - y1) / (y2 - y1));
				}
			}

			intersections.sort_by(|a, b| a.partial_cmp(b).unwrap());

			for pair in intersections.chunks_exact(2) {
				let x1 = ((pair[0] - 0.5).round() as usize).min(self.width - 1);
				let x2 = ((pair[1] - 0.5).round() as usize).min(self.width - 1);

				self.data[y * self.width..][..self.width][x1..=x2].fill(item);
			}

			intersections.clear();
		}
	}
}
