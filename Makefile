# Bifrost top-level Makefile
# Convenience targets for build, tests, sanity checks, and benchmarks

.PHONY: all build test sanity sanity_strategy sanity_e2e sanity_all_scenarios sanity_tool benchmark fmt clippy clean

ROOT := $(CURDIR)
CARGO := cargo
RUSTFLAGS :=
RUBY := ruby
NC := nc

all: build

build:
	$(CARGO) build

test:
	$(CARGO) test --all --all-features

fmt:
	$(CARGO) fmt --all

clippy:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

benchmark:
	cd $(ROOT)/tests/benchmark && ./run_benchmark.sh

clean:
	$(CARGO) clean

