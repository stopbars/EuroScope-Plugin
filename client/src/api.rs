#![allow(private_interfaces)]

use crate::context::Context as ContextImpl;
use crate::screen::Screen as ScreenImpl;
use crate::{ActivityState, ConnectionState};

use std::ffi::{c_char, CStr, CString};

struct Context {
	ctx: ContextImpl,
	string: Option<CString>,
}

struct Screen {
	screen: ScreenImpl<'static>,
	string: Option<CString>,
	strings: Vec<CString>,
	string_ptrs: Vec<*const c_char>,
}

impl Screen {
	fn load_strings(&mut self, strings: Vec<String>) -> *const *const c_char {
		self.strings.clear();
		self.string_ptrs.clear();

		for string in strings {
			let string = unsafe { CString::from_vec_unchecked(string.into_bytes()) };
			self.string_ptrs.push(string.as_ptr());
			self.strings.push(string);
		}

		self.string_ptrs.push(std::ptr::null());

		self.string_ptrs.as_ptr()
	}
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

#[no_mangle]
pub extern "C" fn client_create_screen(
	ctx: &'static mut Context,
	geo: bool,
) -> *mut Screen {
	Box::leak(Box::new(Screen {
		screen: ctx.ctx.create_screen(geo),
		string: None,
		strings: Vec::new(),
		string_ptrs: Vec::new(),
	}))
}

#[no_mangle]
pub unsafe extern "C" fn client_delete_screen(screen: *mut Screen) {
	let _ = Box::from_raw(screen);
}

#[no_mangle]
pub extern "C" fn client_get_aerodrome(screen: &mut Screen) -> *const c_char {
	if let Some(icao) = screen.screen.aerodrome() {
		let string =
			unsafe { CString::from_vec_unchecked(icao.as_bytes().to_vec()) };
		let ptr = string.as_ptr();
		screen.string = Some(string);
		ptr
	} else {
		std::ptr::null()
	}
}

#[no_mangle]
pub unsafe extern "C" fn client_set_aerodrome(
	screen: &mut Screen,
	icao: *const c_char,
) {
	if icao.is_null() {
		screen.screen.set_aerodrome(None);
	} else {
		let Ok(icao) = CStr::from_ptr(icao).to_str() else {
			return
		};

		screen.screen.set_aerodrome(Some(icao));
	}
}

#[no_mangle]
pub extern "C" fn client_get_activity(screen: &mut Screen) -> ActivityState {
	screen.screen.state()
}

#[no_mangle]
pub extern "C" fn client_set_activity(
	screen: &mut Screen,
	state: ActivityState,
) {
	screen.screen.set_state(state);
}

#[no_mangle]
pub extern "C" fn client_get_profiles(
	screen: &mut Screen,
) -> *const *const c_char {
	screen.load_strings(screen.screen.profiles())
}

#[no_mangle]
pub extern "C" fn client_get_profile(screen: &mut Screen) -> usize {
	screen.screen.profile()
}

#[no_mangle]
pub extern "C" fn client_set_profile(screen: &mut Screen, i: usize) {
	screen.screen.set_profile(i);
}

#[no_mangle]
pub extern "C" fn client_get_presets(
	screen: &mut Screen,
) -> *const *const c_char {
	screen.load_strings(screen.screen.presets())
}

#[no_mangle]
pub extern "C" fn client_apply_preset(screen: &mut Screen, i: usize) {
	screen.screen.apply_preset(i);
}

#[no_mangle]
pub extern "C" fn client_get_views(
	screen: &mut Screen,
) -> *const *const c_char {
	screen.load_strings(screen.screen.views())
}

#[no_mangle]
pub extern "C" fn client_get_view(screen: &mut Screen) -> usize {
	screen.screen.view()
}

#[no_mangle]
pub extern "C" fn client_set_view(screen: &mut Screen, i: usize) {
	screen.screen.set_view(i);
}

#[no_mangle]
pub unsafe extern "C" fn client_is_pilot_enabled(
	screen: &mut Screen,
	callsign: *const c_char,
) -> bool {
	let Ok(callsign) = CStr::from_ptr(callsign).to_str() else {
		return false
	};

	screen.screen.is_pilot_enabled(callsign)
}
