.DEFAULT_GOAL := help

APP := target/Enso.app
LSREGISTER := /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

.PHONY: help run check test build.macos app app.debug quicklook install clean

help: ## List the targets
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_.-]+:.*## / {printf "  make %-12s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

run: ## Run Enso from source
	cargo run

check: ## Type-check without building
	cargo check

test: ## Run the app's and the Quick Look renderer's tests
	cargo test --bin enso
	CARGO_TARGET_DIR=target/quicklook cargo test --manifest-path macos/quicklook/Cargo.toml

build.macos: ## Release build, Enso.app with Quick Look, signature and DMG
	./scripts/build-mac.sh

app: ## Release Enso.app (with Quick Look), no DMG
	./scripts/bundle-macos.sh

app.debug: ## Quick unoptimized Enso.app for trying changes
	./scripts/bundle-macos.sh --debug

quicklook: ## Rebuild only the Quick Look extension inside an existing Enso.app
	@test -d $(APP) || { echo "No $(APP) yet: run 'make app' first."; exit 1; }
	./scripts/build-quicklook.sh $(APP)
	codesign --force --sign - $(APP)

install: ## Copy Enso.app to /Applications and register it and its Quick Look preview
	@test -d $(APP) || { echo "No $(APP) yet: run 'make app' first."; exit 1; }
	rm -rf /Applications/Enso.app
	cp -R $(APP) /Applications/
	$(LSREGISTER) -f /Applications/Enso.app
	pluginkit -a /Applications/Enso.app/Contents/PlugIns/EnsoQuickLook.appex
	qlmanage -r >/dev/null 2>&1 || true

clean: ## Remove build output
	cargo clean
