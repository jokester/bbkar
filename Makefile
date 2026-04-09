watch-test:
	exec cargo watch -x test

watch-check:
	exec cargo watch

dryrun:
	RUST_BACKTRACE=1 cargo run -- dryrun --config examples/local.toml

config:
	RUST_BACKTRACE=1 cargo run -- config --config examples/local.toml
 
run:
	RUST_BACKTRACE=1 cargo run -- run --config examples/local.toml

build:
	cargo build
