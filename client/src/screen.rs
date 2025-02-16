use crate::context::Context;
use crate::ActivityState;

use tracing::warn;

pub struct Screen<'a> {
	context: &'a mut Context,
	icao: Option<String>,
	geo: bool,
}

impl<'a> Screen<'a> {
	pub fn new(context: &'a mut Context, geo: bool) -> Self {
		Self {
			context,
			icao: None,
			geo,
		}
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
			.icao
			.as_ref()
			.and_then(|icao| {
				self
					.context
					.client()
					.and_then(|client| client.aerodrome(icao))
					.map(|aerodrome| aerodrome.state())
			})
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
}

impl<'a> Drop for Screen<'a> {
	fn drop(&mut self) {
		if let Some(icao) = &self.icao {
			self.context.untrack_aerodrome(icao);
		}
	}
}
