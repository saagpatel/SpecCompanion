.PHONY: build test lint clean check run

build:
	cargo build --locked --manifest-path src-tauri/Cargo.toml --release

check:
	cargo check --locked --manifest-path src-tauri/Cargo.toml

test:
	cargo test --locked --manifest-path src-tauri/Cargo.toml

lint:
	cargo clippy --locked --manifest-path src-tauri/Cargo.toml -- -D warnings

run:
	cargo run --locked --manifest-path src-tauri/Cargo.toml

clean:
	cargo clean --manifest-path src-tauri/Cargo.toml
