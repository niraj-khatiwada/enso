.DEFAULT_GOAL := help

APP := target/enso.app
LSREGISTER := /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

.PHONY: help run check test bench build.macos app app.debug quicklook install install.linux release clean

help: ## List the targets
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_.-]+:.*## / {printf "  make %-12s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

run: ## Run enso from source
	cargo run

check: ## Type-check without building
	cargo check

test: ## Run the app's and the Quick Look renderer's tests
	cargo test --bin enso
	CARGO_TARGET_DIR=target/quicklook cargo test --manifest-path macos/quicklook/Cargo.toml

bench: ## Visual editor benchmark suite (1k, 50k, 200k, 255k), prints result tables
	./scripts/bench.sh

release: ## Tag and publish a release: make release VERSION=1.2.3
	@test -n "$(VERSION)" || { echo "Usage: make release VERSION=1.2.3"; exit 1; }
	./scripts/release.sh $(VERSION)

build.macos: ## Release build, enso.app with Quick Look, signature and DMG
	./scripts/build-mac.sh

app: ## Release enso.app (with Quick Look), no DMG
	./scripts/bundle-macos.sh

app.debug: ## Quick unoptimized enso.app for trying changes
	./scripts/bundle-macos.sh --debug

quicklook: ## Rebuild only the Quick Look extension inside an existing enso.app
	@test -d $(APP) || { echo "No $(APP) yet: run 'make app' first."; exit 1; }
	./scripts/build-quicklook.sh $(APP)
	codesign --force --sign - $(APP)

install: ## Copy enso.app to /Applications and register it and its Quick Look preview
	@test -d $(APP) || { echo "No $(APP) yet: run 'make app' first."; exit 1; }
	rm -rf /Applications/Enso.app /Applications/enso.app
	cp -R $(APP) /Applications/
	$(LSREGISTER) -f /Applications/enso.app
	pluginkit -a /Applications/enso.app/Contents/PlugIns/EnsoQuickLook.appex
	qlmanage -r >/dev/null 2>&1 || true

install.linux: ## Linux: add enso to the app launcher, with its icon (after cargo build)
	./scripts/install-linux.sh

clean: ## Remove build output
	cargo clean
