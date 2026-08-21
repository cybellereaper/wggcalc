.PHONY: build run test update-data clean

build:
	cargo build --release

run:
	cargo run --bin wggcalc --

test:
	cargo test

update-data:
	cargo run --release --bin parse_sheet

clean:
	cargo clean
