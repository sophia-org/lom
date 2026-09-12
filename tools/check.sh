#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 -B -m unittest discover -s "$root/tools/tests" -p 'test_*.py'
python3 -B "$root/tools/audit_source_layout.py"
