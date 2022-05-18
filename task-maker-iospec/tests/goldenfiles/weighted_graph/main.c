#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
#include <assert.h>

int main() {
    int N;
    int M;
    int* W;
    int* A;
    int* B;
    int S;
    int* X;

    /** Number of nodes in the graph */
    /** Second single-line doc comment */
    /**
        Block doc comment
     */
    /** Block doc comment
        Block doc comment
     */
    assert(scanf("%d", &N) == 1);
    assert(scanf("%d", &M) == 1);
    assert(2 <= N && N < 100000);
    assert(0 <= M && M < 500000);
    W = malloc(sizeof(int) * (N));
    for(int u = 0; u < N; u++) {
        assert(scanf("%d", &W[u]) == 1);
        assert(0 <= W[u] && W[u] < 1000000000);
    }
    A = malloc(sizeof(int) * (M));
    B = malloc(sizeof(int) * (M));
    for(int i = 0; i < M; i++) {
        assert(scanf("%d", &A[i]) == 1);
        assert(scanf("%d", &B[i]) == 1);
        assert(0 <= A[i] && A[i] < N);
        assert(0 <= B[i] && B[i] < N);
    }
    S = 42;
    /** Answer */
    printf("%d ", S);
    printf("\n");
    X = malloc(sizeof(int) * (N));
    for(int u = 0; u < N; u++) {
        X[u] = 10 + u;
        printf("%d ", X[u]);
    }
    printf("\n");
}
