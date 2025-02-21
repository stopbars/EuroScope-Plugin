mod api;
mod client;
mod config;
mod context;
mod ipc;
mod screen;
mod server;

use serde::{Deserialize, Serialize};

pub use api::*;

#[derive(
	Clone,
	Copy,
	Debug,
	Hash,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Deserialize,
	Serialize,
)]
#[repr(C)]
pub enum ConnectionState {
	Disconnected,
	ConnectedDirect,
	ConnectedProxy,
	ConnectedLocal,
	Poisoned,
}

#[derive(
	Clone,
	Copy,
	Debug,
	Hash,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Deserialize,
	Serialize,
)]
#[repr(C)]
pub enum ActivityState {
	None,
	Observing,
	Controlling,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct ViewportGeo {
	origin: [f64; 2],
	scaling: [f64; 2],
	rotation: f64,
	size: [f64; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct ViewportNonGeo {
	origin: [f64; 2],
	size: [f64; 2],
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
pub enum ClickType {
	Primary,
	Auxiliary,
}
