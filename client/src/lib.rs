mod api;
mod client;
mod config;
mod context;
mod ipc;
mod server;

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

#[repr(C)]
pub enum ActivityState {
	None,
	Observing,
	Controlling,
}
