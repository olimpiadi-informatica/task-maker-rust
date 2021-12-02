#include <signal.h>

#include <cassert>
#include <cstdio>
#include <cstdlib>

using namespace std;

int main(int argc, char **argv) {
    signal(SIGPIPE, SIG_IGN);

    FILE *fin, *fifo_in1, *fifo_out1, *fifo_in2, *fifo_out2;

    fin = fopen("input.txt", "r");
    fifo_in1 = fopen(argv[1], "w");
    fifo_out1 = fopen(argv[2], "r");
    fifo_in2 = fopen(argv[3], "w");
    fifo_out2 = fopen(argv[4], "r");

    int a, b, c, res;
    assert(3 == fscanf(fin, "%d %d %d", &a, &b, &c));

    fprintf(fifo_in1, "%d %d\n", a, b);
    fflush(fifo_in1);
    fscanf(fifo_out1, "%d", &res);

    fprintf(fifo_in2, "%d %d\n", res, c);
    fflush(fifo_in2);
    fscanf(fifo_out2, "%d", &res);

    if ((a + b) * c == res) {
        fprintf(stderr, "Ok!\n");
        printf("1.0\n");
    } else {
        fprintf(stderr, "Ko!\n");
        printf("0.0\n");
    }
}

