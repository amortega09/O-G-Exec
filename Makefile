SHELL := /bin/bash
CARGO := $(HOME)/.cargo/bin/cargo

.PHONY: all wasm web dev test sync-data clean

## Build everything: compile WASM then start dev server
all: wasm web

## Run all Rust tests
test:
	$(CARGO) test --workspace

## Build the WASM bridge (outputs to web/pkg/)
wasm:
	$(CARGO) install wasm-pack 2>/dev/null || true
	$(HOME)/.cargo/bin/wasm-pack build crates/wasm-bridge --target web --out-dir ../../web/pkg

## Install web dependencies
web:
	cd web && npm install

## Run the dev server (assumes WASM is built and web deps installed)
dev:
	cd web && npm run dev

## Sync canonical game data (data/) into the web app's served public/data dir.
## Runs automatically via npm pre-scripts; this target is for manual use.
sync-data:
	cd web && node scripts/sync-data.mjs

## Clean build artifacts
clean:
	$(CARGO) clean
	rm -rf web/pkg web/node_modules web/dist
