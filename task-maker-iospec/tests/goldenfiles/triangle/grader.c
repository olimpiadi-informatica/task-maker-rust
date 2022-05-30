#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
#include <assert.h>

int main() {
    int N = 0;
    int** A = 0;

    assert(scanf("%d", &N) == 1);
    A = realloc(A, sizeof(int*) * (N));
    for(int i = 0; i < N; i++) {
        A[i] = realloc(A[i], sizeof(int) * (i + 1));
        for(int j = 0; j < i + 1; j++) {
            assert(scanf("%d", &A[i][j]) == 1);
        }
    }
}
