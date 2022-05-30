#ifndef IOLIB_HPP
#define IOLIB_HPP

#include <vector>
#include <functional>

using std::vector;


int f(int N, int M);
void g(int N, int& M, vector<int>& A, vector<int> B, vector<int>& X);
struct IoData {
    int N = {};
    int M = {};
    vector<int> W = {};
    vector<int> A = {};
    vector<int> B = {};
    int S = {};
    vector<int> X = {};

    struct Funs {
        std::function<int(int N, int M)> f = [](auto...) { return 0; };
        std::function<void(int N, int& M, vector<int>& A, vector<int> B, vector<int>& X)> g = [](auto...) {};
    };

    static Funs global_funs() {
        Funs funs;
        funs.f = f;
        funs.g = g;
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
    auto& f = funs.f;
    auto& g = funs.g;
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
    invoke(S, f, N, M);
    /** Answer */
    item(OUTPUT, S);
    endl(OUTPUT);
    invoke_void(g, N, M, A, B, X);
    resize(OUTPUT, X, N);
    for(int u = 0; u < N; u++) {
        item(OUTPUT, X[u]);
    }
    endl(OUTPUT);
}

#endif
