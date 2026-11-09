.PHONY: build test lint clean check run verify living-spec-layers living-spec-test living-spec-score living-spec-refresh

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

living-spec-layers:
	sh living-spec/scripts/check-layers.sh
	ruby living-spec/scripts/test-ci.rb

living-spec-test: living-spec-layers
	cd living-spec && cargo fmt -- --check
	cd living-spec && cargo clippy --offline --locked --all-targets -- -D warnings
	# CI pins the crate's MSRV (1.95); lint with it too when installed so CI-only lints surface locally.
	if rustup toolchain list 2>/dev/null | grep -q '^1\.95\.0'; then cd living-spec && CARGO_TARGET_DIR=target/msrv cargo +1.95.0 clippy --offline --locked --all-targets -- -D warnings; fi
	mkdir -p living-spec/target
	cd living-spec && CARGO_TERM_COLOR=never NO_COLOR=1 bash -o pipefail -c 'cargo test --offline --locked 2>&1 | tee target/test-output.txt'

# Scores a scratch copy so verification never rewrites the committed dogfood artifacts.
living-spec-score: living-spec-test
	rm -rf living-spec/target/dogfood && cp -R living-spec/living-spec living-spec/target/dogfood
	cd living-spec && cargo run --offline --locked -q -- score target/dogfood --results target/test-output.txt --format libtest --tests . --html target/dogfood/spec.html

# Refreshes the committed dogfood status, history and page after a complete score.
living-spec-refresh: living-spec-test
	bash living-spec/scripts/refresh-dogfood.sh "$(CURDIR)" living-spec/target/test-output.txt

verify: living-spec-score
