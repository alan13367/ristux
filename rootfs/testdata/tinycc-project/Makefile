app: main.o util.o
	$(CC) $^ -o $@

main.o: main.c util.h
	$(CC) -c $< -o $@

util.o: util.c util.h
	$(CC) -c $< -o $@
