# Build the Rust project
.PHONY: rust-build
rust-build:
	cargo build --release

# Build the Rust documentation
rust-doc:
	cargo doc --no-deps --open

# Lint checks for Rust code
.PHONY: lint
lint:
	cargo fmt --all -- --check
	cargo clippy -p era_revm -Zunstable-options -- -D warnings --allow clippy::unwrap_used

# Fix lint errors for Rust code
lint-fix:
	cargo clippy --fix
	cargo fmt

# Run unit tests for Rust code
.PHONY: test
test:
	cargo test
