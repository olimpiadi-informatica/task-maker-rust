#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
#include <assert.h>

int f(int N, int M);
void g(int N, int* M, int* A, int* B, int* X);

int main() {
    int N = 0;
    int M = 0;
    int* W = 0;
    int* A = 0;
    int* B = 0;
    int S = 0;
    int* X = 0;

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
    W = realloc(W, sizeof(int) * (N));
    for(int u = 0; u < N; u++) {
        assert(scanf("%d", &W[u]) == 1);
        assert(0 <= W[u] && W[u] < 1000000000);
    }
    A = realloc(A, sizeof(int) * (M));
    B = realloc(B, sizeof(int) * (M));
    for(int i = 0; i < M; i++) {
        assert(scanf("%d", &A[i]) == 1);
        assert(scanf("%d", &B[i]) == 1);
        assert(0 <= A[i] && A[i] < N);
        assert(0 <= B[i] && B[i] < N);
    }
    S = f(N, M);
    /** Answer */
    printf("%d ", S);
    printf("\n");
    X = realloc(X, sizeof(int) * (N));
    g(N, &M, A, B, X);
    X = realloc(X, sizeof(int) * (N));
    for(int u = 0; u < N; u++) {
        printf("%d ", X[u]);
    }
    printf("\n");
}
