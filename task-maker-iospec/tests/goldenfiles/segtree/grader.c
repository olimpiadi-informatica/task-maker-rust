#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
#include <assert.h>

int main() {
    int n = 0;
    int q = 0;
    int* a = 0;

    assert(scanf("%d", &n) == 1);
    assert(scanf("%d", &q) == 1);
    a = realloc(a, sizeof(int) * (n));
    for(int i = 0; i < n; i++) {
        assert(scanf("%d", &a[i]) == 1);
    }
    for(int i = 0; i < q; i++) {
        int op = 0;
        int l1 = 0;
        int r1 = 0;
        int l2 = 0;
        int r2 = 0;
        int x2 = 0;
        int l3 = 0;
        int r3 = 0;
        int x3 = 0;
        int l4 = 0;
        int r4 = 0;
        int l5 = 0;
        int r5 = 0;
        int x5 = 0;
        int s = 0;

        assert(scanf("%d", &op) == 1);
        if(op == 1) {
            assert(scanf("%d", &l1) == 1);
            assert(scanf("%d", &r1) == 1);
        }
        if(op == 2) {
            assert(scanf("%d", &l2) == 1);
            assert(scanf("%d", &r2) == 1);
            assert(scanf("%d", &x2) == 1);
        }
        if(op == 3) {
            assert(scanf("%d", &l3) == 1);
            assert(scanf("%d", &r3) == 1);
            assert(scanf("%d", &x3) == 1);
        }
        if(op == 4) {
            assert(scanf("%d", &l4) == 1);
            assert(scanf("%d", &r4) == 1);
        }
        if(op == 5) {
            assert(scanf("%d", &l5) == 1);
            assert(scanf("%d", &r5) == 1);
            assert(scanf("%d", &x5) == 1);
        }
        printf("%d ", s);
        printf("\n");
    }
}
