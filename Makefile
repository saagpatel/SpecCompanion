.PHONY: build test lint clean check run verify living-spec-layers living-spec-test living-spec-score living-spec-refresh living-spec-mutate living-spec-score-mutation

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
	@if test -f living-spec/living-spec/mutation.json; then ruby living-spec/scripts/report-mutation-binding.rb living-spec/living-spec/status.json living-spec/living-spec/mutation.json; fi

# Refreshes the committed dogfood status, history and page after a complete score.
living-spec-refresh: living-spec-test
	bash living-spec/scripts/refresh-dogfood.sh "$(CURDIR)" living-spec/target/test-output.txt

# Explicitly opt-in: mutation execution rebuilds the crate for each selected mutant.
living-spec-mutate:
	cd living-spec && CARGO_TERM_COLOR=never NO_COLOR=1 cargo run --offline --locked -q -- mutate living-spec --tests . --jobs 2 --out living-spec/mutation.json $(MUTATE_ARGS)

# Score a scratch copy so this target never rewrites committed dogfood status.
living-spec-score-mutation: living-spec-test
	rm -rf living-spec/target/dogfood-mutation && cp -R living-spec/living-spec living-spec/target/dogfood-mutation
	cd living-spec && cargo run --offline --locked -q -- score target/dogfood-mutation --results target/test-output.txt --format libtest --tests . --mutation --html target/dogfood-mutation/spec.html
	ruby -rjson -e 's=JSON.parse(File.read(ARGV.fetch(0))); c=Hash.new(0); s.fetch("statuses").each { |x| c[x.dig("mutation", "state") || "none"] += 1 }; puts "mutation_checked: #{c["mutation_checked"]}, survived: #{c["survived"]}, stale: #{c["stale"]}"' living-spec/target/dogfood-mutation/status.json

verify: living-spec-score
