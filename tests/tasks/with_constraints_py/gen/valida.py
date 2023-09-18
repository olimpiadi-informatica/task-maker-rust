#!/usr/bin/env python3

# pylint: disable=wildcard-import
# pylint: disable=invalid-name

import sys
from constraints import *

def run(file: list[str], st: int):
    assert len(file) == 1
    N = int(file[0].strip())
    assert 1 <= N <= subtasks['MAXN']

assert len(sys.argv) >= 2
file = open(sys.argv[1]).read().splitlines()

st = 0
if len(sys.argv) >= 3:
    st = int(sys.argv[2])

run(file, st)
