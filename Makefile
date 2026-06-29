PREFIX ?= /usr
DESTDIR ?=

BIN := target/release/minazuki

build:
	cargo build --release

install:
	@test -f $(BIN) || { echo "minazuki: run 'make build' first (as your user, not root)"; exit 1; }
	install -Dm755 $(BIN) $(DESTDIR)$(PREFIX)/bin/minazuki
	install -Dm644 packaging/minazuki.service $(DESTDIR)$(PREFIX)/lib/systemd/system/minazuki.service

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/minazuki
	rm -f $(DESTDIR)$(PREFIX)/lib/systemd/system/minazuki.service

.PHONY: build install uninstall
