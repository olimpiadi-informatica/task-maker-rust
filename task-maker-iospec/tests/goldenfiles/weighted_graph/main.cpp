#include <vector>
#include <iostream>
#include <cassert>

using namespace std;

int main() {
    int N;
    int M;
    vector<int> W;
    vector<int> A;
    vector<int> B;
    int S;
    vector<int> X;

    /** Number of nodes in the graph */
    /** Second single-line doc comment */
    /**
        Block doc comment
     */
    /** Block doc comment
        Block doc comment
     */
    std::cin >> N;
    std::cin >> M;
    assert(2 <= N && N < 100000);
    assert(0 <= M && M < 500000);
    W.resize(N);
    for(int u = 0; u < N; u++) {
        std::cin >> W[u];
        assert(0 <= W[u] && W[u] < 1000000000);
    }
    A.resize(M);
    B.resize(M);
    for(int i = 0; i < M; i++) {
        std::cin >> A[i];
        std::cin >> B[i];
        assert(0 <= A[i] && A[i] < N);
        assert(0 <= B[i] && B[i] < N);
    }
    S = 42;
    /** Answer */
    std::cout << S << " ";
    std::cout << std::endl;
    X.resize(N);
    for(int u = 0; u < N; u++) {
        X[u] = 10 + u;
        std::cout << X[u] << " ";
    }
    std::cout << std::endl;
}
