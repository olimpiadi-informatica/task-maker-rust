#include <vector>



struct IoData
{
    int N = {};
    int M = {};
    std::vector<int> W = {};
    std::vector<int> A = {};
    std::vector<int> B = {};
    int S = {};
    std::vector<int> X = {};
}

;

template <typename Input, typename Output, typename Check>
void process_io(
    IoData& data = {},
    Input input_item = [](auto&) {},
    Output output_item = [](auto&) {},
    Check check_expr = [](auto) {}
)
{
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
    input_item(N);
    input_item(M);
    check_expr(2 <= N && N < 100000);
    check_expr(0 <= M && M < 500000);
    W.resize(N);
    for(int u = 0; u < N; u++)
    {
        input_item(W[u]);
        check_expr(0 <= W[u] && W[u] < 1000000000);
    }
    A.resize(M);
    B.resize(M);
    for(int i = 0; i < M; i++)
    {
        input_item(A[i]);
        input_item(B[i]);
        check_expr(0 <= A[i] && A[i] < N);
        check_expr(0 <= B[i] && B[i] < N);
    }
    S = 42;
    /** Answer */
    output_item(S);
    X.resize(N);
    for(int u = 0; u < N; u++)
    {
        X[u] = 10 + u;
        output_item(X[u]);
    }
}

