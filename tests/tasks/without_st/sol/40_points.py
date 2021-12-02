#!/usr/bin/env python3

from __future__ import print_function

with open("input.txt") as f:
    x = int(f.read().splitlines()[0])
    if x >= 100:
        x = -1234
    print(x, file=open("output.txt", "w"))
