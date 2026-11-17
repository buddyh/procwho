.PHONY: build test app install install-app uninstall

build:
	cargo build --release

test:
	cargo test --workspace
	cd app && swift build && swift test

app: build
	app/scripts/bundle.sh

install: build
	scripts/install.sh cli

install-app: app
	scripts/install.sh app

uninstall:
	scripts/install.sh uninstall
