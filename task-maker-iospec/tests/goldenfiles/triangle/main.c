#include <stdio.h>
#include <stdlib.h>
#include <assert.h>

int main() {
    int N;
    int** A;

    assert(scanf("%d", &N) == 1);
    A = malloc(sizeof(int*) * (N));
    for(int i = 0; i < N; i++) {
        A[i] = malloc(sizeof(int) * (i + 1));
        for(int j = 0; j < i + 1; j++) {
            assert(scanf("%d", &A[i][j]) == 1);
        }
    }
}
