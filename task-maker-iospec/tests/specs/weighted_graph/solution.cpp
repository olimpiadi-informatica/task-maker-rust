#include <vector>

using namespace std;

int f(int N, int M) {
    return 42;
}

void g(int N, int& M, vector<int>& A, vector<int> B, vector<int>& X) {
    for(int i = 0; i < N; i++) X[i] = i;
}
