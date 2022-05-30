#include <vector>
#include <iostream>
#include <cassert>

using namespace std;

int main() {
    int N;
    vector<vector<int>> A;

    std::cin >> N;
    A.resize(N);
    for(int i = 0; i < N; i++) {
        A[i].resize(i + 1);
        for(int j = 0; j < i + 1; j++) {
            std::cin >> A[i][j];
        }
    }
}
