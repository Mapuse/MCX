include env.mk

PROFILE ?= release
TARGET  := target/$(RUST_TARGET)/$(PROFILE)/mcx
DESTDIR  ?=

# ── cps: external package, fetched from upstream ─────────────────────────
CPS_URL ?= https://github.com/Mapuse/CPS
CPS_DIR ?= $(HOME)/cudane-deps/cps
CPS_REF ?= c4ba21e185398558052acec3f0b4619b4e8c0678

$(CPS_DIR):
	git clone $(CPS_URL) $(CPS_DIR)
	git -C $(CPS_DIR) checkout $(CPS_REF)

.PHONY: all build deps install install-man clean uninstall

all: build

deps: $(CPS_DIR)

build: $(CPS_DIR)
	CARGO_TARGET_DIR=$(CURDIR)/target cargo build --target $(RUST_TARGET) --profile $(PROFILE) --locked

install: build install-man
	install -Dm755 $(TARGET) $(DESTDIR)$(PREFIX)/bin/mcx

install-man:
	install -d $(DESTDIR)$(PREFIX)/share/man/man1
	install -m 644 docs/mcx.1 $(DESTDIR)$(PREFIX)/share/man/man1/

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/mcx

clean:
	cargo clean
