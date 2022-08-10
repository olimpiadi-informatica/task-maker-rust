#!/usr/bin/env python3

import sys

with open(sys.argv[1], "r") as f:
    a, b, c = map(int, f.read().strip().split())
    assert a in {0, 1}
