static HELLO_WORLD: &[u8] = b"Hello, world!\0";

#[no_mangle]
pub extern "C" fn hello() -> *const u8 {
	HELLO_WORLD.as_ptr()
}
