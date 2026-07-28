.PHONY: all precommit lint format coverage clean

all: precommit

# ─── Pre-commit (read-only checks) ───────────────────────────────────

precommit:
	bash scripts/precommit.sh

# ─── Lint (read-only, used by precommit) ─────────────────────────────

lint: check clippy fmt-check

check:
	RUSTFLAGS="-D warnings" cargo check

clippy:
	cargo clippy --all-targets -- -D warnings

fmt-check:
	cargo fmt --check

# ─── Format (write-mode) ─────────────────────────────────────────────

format:
	cargo fmt

ci: lint coverage

# ─── Coverage ─────────────────────────────────────────────────────────

coverage:
	mkdir -p coverage && \
	if [ -x /usr/bin/llvm-cov ] && [ -x /usr/bin/llvm-profdata ]; then \
		export LLVM_COV=/usr/bin/llvm-cov LLVM_PROFDATA=/usr/bin/llvm-profdata; \
	fi && \
	CARGO_TARGET_DIR=target RUSTFLAGS="-D warnings" \
	cargo llvm-cov --lcov --output-path coverage/lcov.info \
		--ignore-filename-regex 'src/main\.rs|src/seed\.rs' \
		--fail-under-lines 75 \
		--no-clean --workspace
# ─── Cleanup ──────────────────────────────────────────────────────────

clean:
	cargo clean
	rm -rf coverage/