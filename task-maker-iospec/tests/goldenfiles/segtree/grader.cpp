#include <vector>
#include <iostream>
#include <cassert>

using namespace std;

int main() {
    int n;
    int q;
    vector<int> a;

    std::cin >> n;
    std::cin >> q;
    a.resize(n);
    for(int i = 0; i < n; i++) {
        std::cin >> a[i];
    }
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

        std::cin >> op;
        if(op == 1) {
            std::cin >> l1;
            std::cin >> r1;
        }
        if(op == 2) {
            std::cin >> l2;
            std::cin >> r2;
            std::cin >> x2;
        }
        if(op == 3) {
            std::cin >> l3;
            std::cin >> r3;
            std::cin >> x3;
        }
        if(op == 4) {
            std::cin >> l4;
            std::cin >> r4;
        }
        if(op == 5) {
            std::cin >> l5;
            std::cin >> r5;
            std::cin >> x5;
        }
        std::cout << s << " ";
        std::cout << std::endl;
    }
}
