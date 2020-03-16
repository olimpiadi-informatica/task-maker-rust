#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

int op(int code, int a, int b);

int main(int argc, char **argv) {
    FILE *fifo_in, *fifo_out;
    fifo_in = fopen(argv[2], "r");
    fifo_out = fopen(argv[1], "w");
    int code = atoi(argv[3]);

    int a, b;
    assert(fscanf(fifo_in, "%d %d", &a, &b) == 2);
    fprintf(fifo_out, "%d\n", op(code, a, b));
    fflush(fifo_out);

    fclose(fifo_in);
    fclose(fifo_out);

    return 0;
}
