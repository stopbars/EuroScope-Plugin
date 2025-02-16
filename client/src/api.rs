#![allow(private_interfaces)]

use crate::context::Context as ContextImpl;
use crate::ConnectionState;

use std::ffi::{c_char, CStr, CString};

struct Context {
	ctx: ContextImpl,
	string: Option<CString>,
}

#[no_mangle]
pub unsafe extern "C" fn client_init(dir: *const c_char) -> *mut Context {
	let Ok(dir) = CStr::from_ptr(dir).to_str() else {
		return std::ptr::null_mut()
	};

	if let Some(ctx) = ContextImpl::new(dir) {
		Box::leak(Box::new(Context { ctx, string: None }))
	} else {
		std::ptr::null_mut()
	}
}

#[no_mangle]
pub unsafe extern "C" fn client_exit(ctx: *mut Context) {
	let _ = Box::from_raw(ctx);
}

#[no_mangle]
pub extern "C" fn client_tick(ctx: &mut Context) {
	ctx.ctx.tick();
}

#[no_mangle]
pub unsafe extern "C" fn client_connect_direct(
	ctx: &mut Context,
	callsign: *const c_char,
	controlling: bool,
) {
	let Ok(callsign) = CStr::from_ptr(callsign).to_str() else {
		return
	};

	ctx.ctx.connect_direct(callsign, controlling);
}

#[no_mangle]
pub extern "C" fn client_connect_proxy(ctx: &mut Context) {
	ctx.ctx.connect_proxy();
}

#[no_mangle]
pub extern "C" fn client_connect_local(ctx: &mut Context) {
	ctx.ctx.connect_local();
}

#[no_mangle]
pub extern "C" fn client_disconnect(ctx: &mut Context) {
	ctx.ctx.disconnect();
}

#[no_mangle]
pub extern "C" fn client_connection_state(ctx: &Context) -> ConnectionState {
	ctx.ctx.connection_state()
}

#[no_mangle]
pub extern "C" fn client_next_message(ctx: &mut Context) -> *const c_char {
	if let Some(message) = ctx.ctx.next_message() {
		let string = unsafe { CString::from_vec_unchecked(message.into_bytes()) };
		let ptr = string.as_ptr();
		ctx.string = Some(string);
		ptr
	} else {
		ctx.string = None;
		std::ptr::null()
	}
}
