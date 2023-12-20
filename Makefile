.PHONY: build-zkforge
build-zkforge:
	cargo install --path ./crates/zkforge --profile local --force --locked
