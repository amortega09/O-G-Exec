SHELL := /bin/bash
CARGO := $(HOME)/.cargo/bin/cargo

.PHONY: all wasm web dev test clean

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

## Export the phase0 refinery to JSON for the web app
export-refinery:
	$(CARGO) run -p refinery-lp --example export_refinery > web/public/data/refinery.json

## Clean build artifacts
clean:
	$(CARGO) clean
	rm -rf web/pkg web/node_modules web/dist
