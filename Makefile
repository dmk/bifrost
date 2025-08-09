# Bifrost top-level Makefile
# Convenience targets for build, tests, sanity checks, and benchmarks

.PHONY: all build check test sanity sanity_strategy sanity_e2e sanity_all_scenarios sanity_tool benchmark fmt clippy coverage coverage-html verify clean

ROOT := $(CURDIR)
CARGO := cargo
RUSTFLAGS :=
RUBY := ruby
NC := nc

all: build

build:
	$(CARGO) build

check:
	$(CARGO) check --all --all-features

test:
	$(CARGO) test --all --all-features

fmt:
	$(CARGO) fmt --all

clippy:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

coverage:
	cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info

coverage-html:
	cargo llvm-cov --workspace --all-features --html --open

verify: check test clippy coverage

benchmark:
	cd $(ROOT)/tests/benchmark && ./run_benchmark.sh

clean:
	$(CARGO) clean

