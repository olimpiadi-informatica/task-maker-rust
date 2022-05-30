#ifndef IOLIB_HPP
#define IOLIB_HPP

#include <vector>
#include <functional>

using std::vector;


int gi32(int x);
long long gi64(long long x);
bool gbool(bool x);
struct IoData {
    int xi32 = {};
    long long xi64 = {};
    bool xbool = {};
    int yi32 = {};
    long long yi64 = {};
    bool ybool = {};

    struct Funs {
        std::function<int(int x)> gi32 = [](auto...) { return 0; };
        std::function<long long(long long x)> gi64 = [](auto...) { return 0; };
        std::function<bool(bool x)> gbool = [](auto...) { return 0; };
    };

    static Funs global_funs() {
        Funs funs;
        funs.gi32 = gi32;
        funs.gi64 = gi64;
        funs.gbool = gbool;
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
    auto& xi32 = data.xi32;
    auto& xi64 = data.xi64;
    auto& xbool = data.xbool;
    auto& yi32 = data.yi32;
    auto& yi64 = data.yi64;
    auto& ybool = data.ybool;
    auto& gi32 = funs.gi32;
    auto& gi64 = funs.gi64;
    auto& gbool = funs.gbool;
    const bool INPUT = 0;
    const bool OUTPUT = 1;

    item(INPUT, xi32);
    item(INPUT, xi64);
    item(INPUT, xbool);
    endl(INPUT);
    invoke(yi32, gi32, xi32);
    item(OUTPUT, yi32);
    invoke(yi64, gi64, xi64);
    item(OUTPUT, yi64);
    invoke(ybool, gbool, xbool);
    item(OUTPUT, ybool);
    endl(OUTPUT);
}

#endif
