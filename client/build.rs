fn main() {
	// set linker flags

	let xwin = std::env::var("XWIN").unwrap();

	println!("cargo:rustc-link-search=native={xwin}/crt/lib/x86");
	println!("cargo:rustc-link-search=native={xwin}/sdk/lib/shared/x86");
	println!("cargo:rustc-link-search=native={xwin}/sdk/lib/ucrt/x86");
	println!("cargo:rustc-link-search=native={xwin}/sdk/lib/um/x86");
	println!("cargo:rustc-link-arg=/force:unresolved");

	// generate bindings

	let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let out = concat!("out/", env!("CARGO_PKG_NAME"), ".hpp");

	cbindgen::Builder::new()
		.with_crate(dir)
		.with_namespace("client")
		.generate()
		.map_or_else(
			|error| match error {
				cbindgen::Error::ParseSyntaxError { .. } => {
					eprintln!("no bindings generated");
				},
				e => Err(e).unwrap(),
			},
			|bindings| {
				bindings.write_to_file(out);
			},
		);
}
