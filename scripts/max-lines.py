"""Fail for every file longer than the limit.

Usage: max-lines.py --max N FILE...
"""

import argparse
import sys


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--max", type=int, required=True)
    parser.add_argument("files", nargs="*")
    args = parser.parse_args()
    too_long = []
    for path in args.files:
        with open(path, encoding="utf-8") as file:
            count = sum(1 for _ in file)
        if count > args.max:
            too_long.append(f"{path}: {count} lines (max {args.max})")
    for line in too_long:
        print(line)
    return 1 if too_long else 0


if __name__ == "__main__":
    sys.exit(main())
