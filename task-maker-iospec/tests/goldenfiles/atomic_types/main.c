#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
#include <assert.h>

int main() {
    int xi32;
    long long xi64;
    bool xbool;
    int yi32;
    long long yi64;
    bool ybool;

    assert(scanf("%d", &xi32) == 1);
    assert(scanf("%lld", &xi64) == 1);
    assert(scanf("%d", &xbool) == 1);
    printf("%d ", yi32);
    printf("%lld ", yi64);
    printf("%d ", ybool);
    printf("\n");
}
