# Tortuga developer tasks. Run `make` (or `make help`) to list them.
#
# Works with GNU make on Linux, macOS and Windows (install with `winget install ezwinports.make`,
# or run the same commands from Git Bash). Recipes are single commands so they run in any shell.

APP := tortuga-app
UI := tortuga-ui-egui
WASM_TARGET := wasm32-unknown-unknown

ifeq ($(OS),Windows_NT)
  PLATFORM := windows
else ifeq ($(shell uname -s),Darwin)
  PLATFORM := macos
else
  PLATFORM := linux
endif

# The desktop build needs the GTK 3 headers on Linux (native file dialogs). Without them,
# tasks fall back to what can be built anyway: the engine, the UI crate and its tests
# (egui renders without a window), and the web target.
ifeq ($(PLATFORM),linux)
  DESKTOP_DEPS := $(shell pkg-config --exists gtk+-3.0 2>/dev/null && echo yes)
else
  DESKTOP_DEPS := yes
endif

# Under WSL there is no /dev/dri: Mesa probes for a GPU driver, warns ("failed to create dri2
# screen") and falls back to CPU rendering. The d3d12 driver uses the GPU through WSLg instead.
ifeq ($(PLATFORM),linux)
  IS_WSL := $(shell grep -qi microsoft /proc/version 2>/dev/null && echo yes)
endif
RUN_ENV := $(if $(IS_WSL),GALLIUM_DRIVER=$${GALLIUM_DRIVER:-d3d12} ,)

.DEFAULT_GOAL := help
.PHONY: help setup run run-web build release package startup-time icons test test-core test-ui \
        lint lint-desktop lint-web fmt fmt-check deny verify doc clean

help:
	@echo "Tortuga - make targets ($(PLATFORM), desktop deps: $(if $(DESKTOP_DEPS),found,MISSING - run 'make setup'))"
	@echo ""
	@echo "  Getting started"
	@echo "    setup         Install system libraries, Rust toolchain and cargo tools (once per machine)"
	@echo "    run           Run the desktop app"
	@echo "    run-web       Preview the web build in a browser (experimental, needs trunk)"
	@echo ""
	@echo "  Quality"
	@echo "    verify        Everything CI checks: fmt-check, lint, test, deny"
	@echo "    test          All tests (engine + UI)"
	@echo "    test-core     Engine tests only"
	@echo "    test-ui       UI tests only (no window needed)"
	@echo "    lint          Clippy with warnings as errors (desktop + web targets)"
	@echo "    fmt           Format all code"
	@echo "    deny          Check dependency licences and security advisories"
	@echo ""
	@echo "  Building"
	@echo "    build         Debug build of the desktop app"
	@echo "    release       Optimised build of the desktop app"
	@echo "    package       Installers for this OS in target/packages (needs cargo-packager)"
	@echo "    startup-time  Measure start-up of the optimised build (R-NF-1)"
	@echo "    icons         Render the app icon from crates/tortuga-app/icons/tortuga.svg (needs resvg)"
	@echo "    doc           Build and open the API docs"
	@echo "    clean         Remove build output"

# ---------------------------------------------------------------- setup

setup: setup-$(PLATFORM)

.PHONY: setup-linux setup-macos setup-windows
setup-linux:
	./scripts/setup-linux.sh
setup-macos:
	./scripts/setup-macos.sh
setup-windows:
	powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1

# ---------------------------------------------------------------- run

run: require-desktop-deps
	$(RUN_ENV)cargo run -p $(APP)

run-web:
	trunk serve crates/$(APP)/index.html

# ---------------------------------------------------------------- build

build: require-desktop-deps
	cargo build -p $(APP)

release: require-desktop-deps
	cargo build -p $(APP) --release

# Installers (R-NF-4): .deb and .AppImage on Linux, .msi on Windows, .dmg on macOS.
PACKAGE_FORMATS_linux := deb,appimage
PACKAGE_FORMATS_windows := wix
PACKAGE_FORMATS_macos := dmg

package: release
	cargo packager -p $(APP) --release -f $(PACKAGE_FORMATS_$(PLATFORM))

# Opens the app with your workspace, prints how long start-up took, and quits (R-NF-1).
startup-time: release
	$(RUN_ENV)cargo run -p $(APP) --release -- --startup-time

icons:
	python3 scripts/render-icons.py

doc:
	cargo doc --workspace --no-deps --open

clean:
	cargo clean

# ---------------------------------------------------------------- quality

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

test-core:
	cargo test -p tortuga-core

ifeq ($(DESKTOP_DEPS),yes)
test:
	cargo test --workspace
lint: lint-desktop lint-web
else
# The UI crate and its tests need no system libraries, only the desktop launcher does.
test: test-core test-ui
lint: lint-web
	cargo clippy -p tortuga-core -p $(UI) --all-targets -- -D warnings
	@echo "NOTE: GTK 3 not found - the desktop launcher (tortuga-app) was not linted. Run 'make setup' to fix."
endif

test-ui:
	cargo test -p $(UI)

lint-desktop: require-desktop-deps
	cargo clippy --workspace --all-targets -- -D warnings

lint-web:
	cargo clippy -p $(APP) -p $(UI) --no-default-features --features $(APP)/web --target $(WASM_TARGET) -- -D warnings

deny:
	cargo deny check

verify: fmt-check lint test deny

# ---------------------------------------------------------------- helpers

.PHONY: require-desktop-deps
require-desktop-deps:
ifneq ($(DESKTOP_DEPS),yes)
	$(error GTK 3 development files not found. Run 'make setup' (needs sudo) first)
endif
