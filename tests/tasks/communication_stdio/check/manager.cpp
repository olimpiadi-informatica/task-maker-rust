#include <signal.h>

#include <cassert>
#include <cstdio>
#include <cstdlib>

using namespace std;

int main(int argc, char **argv) {
    signal(SIGPIPE, SIG_IGN);

    FILE *fin, *fifo_in, *fifo_out;

    fin = fopen("input.txt", "r");
    fifo_in = fopen(argv[2], "w");
    fifo_out = fopen(argv[1], "r");

    int a, b, res;
    assert(2 == fscanf(fin, "%d %d", &a, &b));

    fprintf(fifo_in, "%d %d\n", a, b);
    fflush(fifo_in);
    fscanf(fifo_out, "%d", &res);

    if (a + b == res) {
        fprintf(stderr, "Ok!\n");
        printf("1.0\n");
    } else {
        fprintf(stderr, "Ko!\n");
        printf("0.0\n");
    }
}

