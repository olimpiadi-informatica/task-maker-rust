#ifndef IOLIB_HPP
#define IOLIB_HPP

#include <vector>

struct IoData {
    int n = {};
    int q = {};
    std::vector<int> a = {};
};

const bool INPUT = 0;
const bool OUTPUT = 1;

template <typename Item, typename Endl, typename Check = void>
void process_io(IoData& data, Item item, Endl endl, Check check) {
    auto& n = data.n;
    auto& q = data.q;
    auto& a = data.a;
    item(INPUT, n);
    item(INPUT, q);
    endl(INPUT);
    a.resize(n);
    for(int i = 0; i < n; i++) {
        item(INPUT, a[i]);
    }
    endl(INPUT);
    for(int i = 0; i < q; i++) {
        int op;
        int l1;
        int r1;
        int l2;
        int r2;
        int x2;
        int l3;
        int r3;
        int x3;
        int l4;
        int r4;
        int l5;
        int r5;
        int x5;
        int s;

        item(INPUT, op);
        if(op == 1) {
            item(INPUT, l1);
            item(INPUT, r1);
        }
        if(op == 2) {
            item(INPUT, l2);
            item(INPUT, r2);
            item(INPUT, x2);
        }
        if(op == 3) {
            item(INPUT, l3);
            item(INPUT, r3);
            item(INPUT, x3);
        }
        if(op == 4) {
            item(INPUT, l4);
            item(INPUT, r4);
        }
        if(op == 5) {
            item(INPUT, l5);
            item(INPUT, r5);
            item(INPUT, x5);
        }
        endl(INPUT);
        item(OUTPUT, s);
        endl(OUTPUT);
    }
}

#endif
