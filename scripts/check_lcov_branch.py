#!/usr/bin/env python3
import sys


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: scripts/check_lcov_branch.py <lcov.info> <min-branches>", file=sys.stderr)
        return 2

    path = sys.argv[1]
    minimum = float(sys.argv[2])

    total_found = 0
    total_hit = 0
    failures = []
    current_file = None
    file_found = 0
    file_hit = 0

    def finish_file() -> None:
        nonlocal file_found, file_hit
        if not current_file or file_found == 0:
            return
        percent = file_hit / file_found * 100
        if percent + 1e-9 < minimum:
            failures.append((current_file, percent, file_hit, file_found))

    with open(path, "r", encoding="utf-8") as file:
        for raw_line in file:
            line = raw_line.strip()
            if line.startswith("SF:"):
                finish_file()
                current_file = line[3:]
                file_found = 0
                file_hit = 0
            elif line.startswith("BRF:"):
                found = int(line[4:])
                file_found = found
                total_found += found
            elif line.startswith("BRH:"):
                hit = int(line[4:])
                file_hit = hit
                total_hit += hit
            elif line == "end_of_record":
                finish_file()
                current_file = None
                file_found = 0
                file_hit = 0

    finish_file()

    if total_found == 0:
        print("branch coverage check failed: no branch counters found", file=sys.stderr)
        return 1

    if failures:
        print(
            f"per-file branch coverage below {minimum:g}% ({len(failures)} files):",
            file=sys.stderr,
        )
        for filename, percent, hit, found in sorted(failures):
            print(f"  {filename}: {percent:.2f}% ({hit}/{found} branches)", file=sys.stderr)
        return 1

    total_percent = total_hit / total_found * 100
    print(
        f"per-file branch coverage >= {minimum:g}%; "
        f"total branch coverage {total_percent:.2f}% ({total_hit}/{total_found} branches)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
