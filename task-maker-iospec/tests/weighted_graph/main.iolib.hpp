#ifndef IOLIB_HPP
#define IOLIB_HPP

#include <vector>

struct IoData {
    int N = {};
    int M = {};
    std::vector<int> W = {};
    std::vector<int> A = {};
    std::vector<int> B = {};
    int S = {};
    std::vector<int> X = {};
};

const bool INPUT = 0;
const bool OUTPUT = 1;

template <typename Item, typename Endl, typename Check = void>
void process_io(IoData& data, Item item, Endl endl, Check check) {
    auto& N = data.N;
    auto& M = data.M;
    auto& W = data.W;
    auto& A = data.A;
    auto& B = data.B;
    auto& S = data.S;
    auto& X = data.X;
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
    W.resize(N);
    for(int u = 0; u < N; u++) {
        item(INPUT, W[u]);
        check(INPUT, 0 <= W[u] && W[u] < 1000000000);
    }
    endl(INPUT);
    A.resize(M);
    B.resize(M);
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
    X.resize(N);
    for(int u = 0; u < N; u++) {
        X[u] = 10 + u;
        item(OUTPUT, X[u]);
    }
    endl(OUTPUT);
}

#endif
