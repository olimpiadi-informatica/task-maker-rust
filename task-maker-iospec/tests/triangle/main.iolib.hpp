#ifndef IOLIB_HPP
#define IOLIB_HPP

#include <vector>

struct IoData {
    int N = {};
    std::vector<std::vector<int>> A = {};
};

const bool INPUT = 0;
const bool OUTPUT = 1;

template <typename Item, typename Endl, typename Check = void>
void process_io(IoData& data, Item item, Endl endl, Check check) {
    auto& N = data.N;
    auto& A = data.A;
    item(INPUT, N);
    endl(INPUT);
    A.resize(N);
    for(int i = 0; i < N; i++) {
        A[i].resize(i + 1);
        for(int j = 0; j < i + 1; j++) {
            item(INPUT, A[i][j]);
        }
        endl(INPUT);
    }
}

#endif
