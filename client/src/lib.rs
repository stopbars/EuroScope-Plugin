mod api;
mod context;

use context::Context;

pub use api::*;

#[repr(C)]
pub enum ConnectionState {
	Disconnected,
	Connecting,
	ConnectedDirect,
	ConnectedProxy,
	ConnectedLocal,
	Poisoned,
}
