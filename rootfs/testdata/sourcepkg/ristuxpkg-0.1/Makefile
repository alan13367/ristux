.PHONY: all install

all: build/output.txt

build/output.txt: src/message.txt
  install -d build
  cat src/message.txt > build/output.txt
  echo built-from-source >> build/output.txt

install: all
  install -D -m 644 build/output.txt /tmp/pkgroot/share/ristuxpkg/output.txt
