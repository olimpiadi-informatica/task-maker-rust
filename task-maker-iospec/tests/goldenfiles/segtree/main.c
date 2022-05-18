#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
#include <assert.h>

int main() {
    int n;
    int q;
    int* a;

    assert(scanf("%d", &n) == 1);
    assert(scanf("%d", &q) == 1);
    a = malloc(sizeof(int) * (n));
    for(int i = 0; i < n; i++) {
        assert(scanf("%d", &a[i]) == 1);
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
