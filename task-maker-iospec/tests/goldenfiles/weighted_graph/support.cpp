#ifndef IOLIB_HPP
#define IOLIB_HPP

#include <vector>
#include <functional>

using std::vector;

struct IoData {
    int N = {};
    int M = {};
    vector<int> W = {};
    vector<int> A = {};
    vector<int> B = {};
    int S = {};
    vector<int> X = {};

    struct Funs {
    };

    static Funs global_funs() {
        Funs funs;
        return funs;
    }
};

template <
   typename Item,
   typename Endl,
   typename Check,
   typename InvokeVoid,
   typename Invoke,
   typename Resize
>
void process_io(
   IoData& data,
   IoData::Funs funs,
   Item item,
   Endl endl,
   Check check,
   InvokeVoid invoke,
   Invoke invoke_void,
   Resize resize
) {
    auto& N = data.N;
    auto& M = data.M;
    auto& W = data.W;
    auto& A = data.A;
    auto& B = data.B;
    auto& S = data.S;
    auto& X = data.X;
    const bool INPUT = 0;
    const bool OUTPUT = 1;

    /** Number of nodes in the graph */
    /** Second single-line doc comment */
    /**
        Block doc comment
     */
    /** Block doc comment
        Block doc comment
     */
    item(INPUT, N);
    item(INPUT, M);
    endl(INPUT);
    check(INPUT, 2 <= N && N < 100000);
    check(INPUT, 0 <= M && M < 500000);
    resize(INPUT, W, N);
    for(int u = 0; u < N; u++) {
        item(INPUT, W[u]);
        check(INPUT, 0 <= W[u] && W[u] < 1000000000);
    }
    endl(INPUT);
    resize(INPUT, A, M);
    resize(INPUT, B, M);
    for(int i = 0; i < M; i++) {
        item(INPUT, A[i]);
        item(INPUT, B[i]);
        endl(INPUT);
        check(INPUT, 0 <= A[i] && A[i] < N);
        check(INPUT, 0 <= B[i] && B[i] < N);
    }
    S = 42;
    /** Answer */
    item(OUTPUT, S);
    endl(OUTPUT);
    resize(OUTPUT, X, N);
    for(int u = 0; u < N; u++) {
        X[u] = 10 + u;
        item(OUTPUT, X[u]);
    }
    endl(OUTPUT);
}

#endif
