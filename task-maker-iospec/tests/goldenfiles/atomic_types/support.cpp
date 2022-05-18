#ifndef IOLIB_HPP
#define IOLIB_HPP

#include <vector>
#include <functional>

using std::vector;

struct IoData {
    int xi32 = {};
    long long xi64 = {};
    bool xbool = {};
    int yi32 = {};
    long long yi64 = {};
    bool ybool = {};

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
    auto& xi32 = data.xi32;
    auto& xi64 = data.xi64;
    auto& xbool = data.xbool;
    auto& yi32 = data.yi32;
    auto& yi64 = data.yi64;
    auto& ybool = data.ybool;
    const bool INPUT = 0;
    const bool OUTPUT = 1;

    item(INPUT, xi32);
    item(INPUT, xi64);
    item(INPUT, xbool);
    endl(INPUT);
    item(OUTPUT, yi32);
    item(OUTPUT, yi64);
    item(OUTPUT, ybool);
    endl(OUTPUT);
}

#endif
