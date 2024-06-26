build_ios: 
	rustup target add aarch64-apple-ios aarch64-apple-ios-sim
	cargo build --release --target aarch64-apple-ios --target aarch64-apple-ios-sim
	cbindgen --lang c --crate rover-mechanic --output rover-mechanic.h
