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
