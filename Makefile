.PHONY: all precommit lint test format coverage clean

all: precommit

# ─── Pre-commit (read-only checks) ───────────────────────────────────

precommit:
	bash scripts/precommit.sh

# ─── Lint (read-only, used by precommit) ─────────────────────────────

lint: check clippy fmt-check

check:
	cargo check

clippy:
	cargo clippy --all-targets -- -D warnings

fmt-check:
	cargo fmt --check

# ─── Format (write-mode) ─────────────────────────────────────────────

format:
	cargo fmt

# ─── Test ─────────────────────────────────────────────────────────────

test:
	cargo nextest run

# ─── Coverage ─────────────────────────────────────────────────────────

coverage:
	cargo llvm-cov --lcov --output-dir coverage \
		--ignore-filename-regex 'src/main\.rs|src/seed\.rs' \
		--hide-instantiations --fail-func-coverage 90

# ─── CI ──────────────────────────────────────────────────────────────

ci: lint test coverage

# ─── Cleanup ──────────────────────────────────────────────────────────

clean:
	cargo clean
	rm -rf coverage/