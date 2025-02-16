use crate::client::Aerodrome;
use crate::context::Context;
use crate::ActivityState;

use tracing::warn;

pub struct Screen<'a> {
	context: &'a mut Context,
	icao: Option<String>,
	view: Option<usize>,
}

impl<'a> Screen<'a> {
	pub fn new(context: &'a mut Context, geo: bool) -> Self {
		Self {
			context,
			icao: None,
			view: (!geo).then_some(0),
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
			if let Err(err) = c.set_activity(icao.clone(), state) {
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
}

impl<'a> Drop for Screen<'a> {
	fn drop(&mut self) {
		if let Some(icao) = &self.icao {
			self.context.untrack_aerodrome(icao);
		}
	}
}
