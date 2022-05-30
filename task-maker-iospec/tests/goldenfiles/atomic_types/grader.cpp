#include <vector>
#include <iostream>
#include <cassert>

using namespace std;

int gi32(int x);
long long gi64(long long x);
bool gbool(bool x);

int main() {
    int xi32;
    long long xi64;
    bool xbool;
    int yi32;
    long long yi64;
    bool ybool;

    std::cin >> xi32;
    std::cin >> xi64;
    std::cin >> xbool;
    yi32 = gi32(xi32);
    std::cout << yi32 << " ";
    yi64 = gi64(xi64);
    std::cout << yi64 << " ";
    ybool = gbool(xbool);
    std::cout << ybool << " ";
    std::cout << std::endl;
}
