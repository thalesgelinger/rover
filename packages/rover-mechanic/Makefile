ios: 
	rustup target add aarch64-apple-ios aarch64-apple-ios-sim
	cargo build --release --target aarch64-apple-ios 
	cargo build --release --target aarch64-apple-ios-sim
	cbindgen --lang c --crate rover-mechanic --output rover-mechanic.h

android: 
	rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
	cargo ndk --target aarch64-linux-android --platform 31 -- build --release
	cargo ndk --target armv7-linux-androideabi --platform 31 -- build --release
	cargo ndk --target i686-linux-android --platform 31 -- build --release
	cargo ndk --target x86_64-linux-android --platform 31 -- build --release
