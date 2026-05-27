PREFIX = /tmp/pkgroot
CC = cc
CFLAGS = -Iinclude

.PHONY: all install clean

all: build/ristux-hello

build:
	mkdir build

build/main.o: build src/main.c include/message.h
	$(CC) $(CFLAGS) -c src/main.c -o build/main.o

build/message.o: build src/message.c include/message.h
	$(CC) $(CFLAGS) -c src/message.c -o build/message.o

build/ristux-hello: build/main.o build/message.o
	$(CC) build/main.o build/message.o -o build/ristux-hello

install: all
	mkdir $(PREFIX)
	mkdir $(PREFIX)/bin
	mkdir $(PREFIX)/share
	mkdir $(PREFIX)/share/doc
	mkdir $(PREFIX)/share/doc/ristux-hello
	install -m 755 build/ristux-hello $(PREFIX)/bin/ristux-hello
	install -m 644 README.txt $(PREFIX)/share/doc/ristux-hello/README.txt

clean:
	rm -f build/main.o
	rm -f build/message.o
	rm -f build/ristux-hello
