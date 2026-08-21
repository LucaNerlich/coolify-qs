PREFIX ?= /usr/local
CARGO ?= cargo

.PHONY: build install uninstall test plugin-test validate fmt clippy bundle verify-bundle clean

build:
	$(CARGO) build --release

install: build
	install -Dm755 target/release/coolify-qs $(DESTDIR)$(PREFIX)/bin/coolify-qs

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/coolify-qs

bundle:
	scripts/build-bundle.sh

verify-bundle:
	scripts/verify-bundle.sh

test:
	$(CARGO) test --all-targets

plugin-test:
	node omarchy/model.test.mjs

validate:
	omarchy plugin validate .
	qmllint -I "$(OMARCHY_PATH)/shell" omarchy/BarWidget.qml omarchy/Panel.qml

fmt:
	$(CARGO) fmt

clippy:
	$(CARGO) clippy --all-targets -- -D warnings

clean:
	$(CARGO) clean
