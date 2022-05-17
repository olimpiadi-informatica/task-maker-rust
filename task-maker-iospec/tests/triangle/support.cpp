#ifndef IOLIB_HPP
#define IOLIB_HPP

#include <vector>
#include <functional>

using std::vector;


struct IoData {
    int N = {};
    vector<vector<int>> A = {};

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
    auto& A = data.A;
    const bool INPUT = 0;
    const bool OUTPUT = 1;

    item(INPUT, N);
    endl(INPUT);
    resize(INPUT, A, N);
    for(int i = 0; i < N; i++) {
        resize(INPUT, A[i], i + 1);
        for(int j = 0; j < i + 1; j++) {
            item(INPUT, A[i][j]);
        }
        endl(INPUT);
    }
}

#endif
