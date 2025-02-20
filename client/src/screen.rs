use crate::client::Aerodrome;
use crate::context::Context;
use crate::ActivityState;

use crate::{ViewportGeo, ViewportNonGeo};

use bars_config::{Geo, GeoPoint, Point};

use tracing::warn;

#[cfg(windows)]
use windows::Win32::Foundation::POINT;
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::{self, HDC};

pub struct Screen<'a> {
	context: &'a mut Context,
	icao: Option<String>,
	view: Option<usize>,
	transform: Transform,
}

impl<'a> Screen<'a> {
	pub fn new(context: &'a mut Context, geo: bool) -> Self {
		Self {
			context,
			icao: None,
			view: (!geo).then_some(0),
			transform: Transform::new(),
		}
	}

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
		}
	}

	pub fn is_pilot_enabled(&self, callsign: &str) -> bool {
		self
			.data()
			.map(|aerodrome| aerodrome.is_pilot_enabled(callsign))
			.unwrap_or(false)
	}

	#[cfg(windows)]
	pub fn draw_background_geo(&mut self, hdc: HDC, viewport: ViewportGeo) {
		self.transform = Transform::new_geo(viewport);

		let Some(aerodrome) = self.data() else { return };

		//
	}

	#[cfg(windows)]
	pub fn draw_background_non_geo(
		&mut self,
		hdc: HDC,
		viewport: ViewportNonGeo,
	) {
		if let Some(view) = self
			.data()
			.and_then(|data| data.config().views.get(self.view.unwrap()))
		{
			self.transform = Transform::new_view(viewport, view.bounds);
		}

		let Some(aerodrome) = self.data() else { return };
		let Some(view) = aerodrome.config().views.get(self.view.unwrap()) else {
			return
		};

		//
	}

	#[cfg(windows)]
	pub fn draw_foreground(&mut self, _hdc: HDC) {
		//
	}
}

impl<'a> Drop for Screen<'a> {
	fn drop(&mut self) {
		if let Some(icao) = &self.icao {
			self.context.untrack_aerodrome(icao);
		}
	}
}

#[derive(Default)]
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
