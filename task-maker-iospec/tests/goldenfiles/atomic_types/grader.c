#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
#include <assert.h>

int gi32(int x);
long long gi64(long long x);
bool gbool(bool x);

int main() {
    int xi32 = 0;
    long long xi64 = 0;
    bool xbool = 0;
    int yi32 = 0;
    long long yi64 = 0;
    bool ybool = 0;

    assert(scanf("%d", &xi32) == 1);
    assert(scanf("%lld", &xi64) == 1);
    assert(scanf("%d", &xbool) == 1);
    yi32 = gi32(xi32);
    printf("%d ", yi32);
    yi64 = gi64(xi64);
    printf("%lld ", yi64);
    ybool = gbool(xbool);
    printf("%d ", ybool);
    printf("\n");
}
