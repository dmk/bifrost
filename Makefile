# Bifrost top-level Makefile
# Convenience targets for build, tests, sanity checks, and benchmarks

.PHONY: all build check test sanity sanity_strategy sanity_e2e sanity_all_scenarios sanity_tool benchmark fmt clippy coverage coverage-html verify clean benchmark-light_read_heavy benchmark-light_write_heavy benchmark-medium_balanced benchmark-high_read_heavy benchmark-large_values benchmark-stress_test

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
	$(CARGO) llvm-cov --workspace --all-features --lcov --output-path lcov.info

coverage-html:
	$(CARGO) llvm-cov --workspace --all-features --html --open

verify: check test clippy coverage fmt

# Benchmark targets - delegate to tests/benchmark/Makefile
benchmark:
	cd $(ROOT)/tests/benchmark && $(MAKE) run-suite SUITE=all

# Individual benchmark targets using YAML configs
benchmark-%:
	cd $(ROOT)/tests/benchmark && $(MAKE) run-suite SUITE=$*

clean:
	$(CARGO) clean

