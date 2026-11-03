#!/usr/bin/env python3
"""Flip one byte in a copied kernel image to prove signature verification rejects it."""
import sys

path = sys.argv[1]
with open(path, "r+b") as f:
    b = f.read(1)
    assert b, "kernel file is empty"
    f.seek(0)
    f.write(bytes([b[0] ^ 1]))
