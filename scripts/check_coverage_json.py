#!/usr/bin/env python3
import json
import os
import re
import sys


def main() -> int:
    if len(sys.argv) != 3:
        print(
            "usage: scripts/check_coverage_json.py <coverage.json> <min-lines>",
            file=sys.stderr,
        )
        return 2

    path = sys.argv[1]
    minimum = float(sys.argv[2])
    ignore_pattern = os.environ.get("COVERAGE_IGNORE_REGEX")
    ignore = re.compile(ignore_pattern) if ignore_pattern else None

    with open(path, "r", encoding="utf-8") as file:
        payload = json.load(file)

    failures = []
    checked = 0
    for file_data in payload.get("data", [{}])[0].get("files", []):
        filename = file_data.get("filename", "")
        relative = relative_src_path(filename)
        if ignore and ignore.search(relative):
            continue

        lines = file_data.get("summary", {}).get("lines", {})
        count = int(lines.get("count", 0))
        if count == 0:
            continue

        checked += 1
        percent = float(lines.get("percent", 0.0))
        if percent + 1e-9 < minimum:
            covered = int(lines.get("covered", 0))
            failures.append((relative, percent, covered, count))

    if checked == 0:
        print("coverage check failed: no files matched the coverage scope", file=sys.stderr)
        return 1

    if failures:
        print(
            f"per-file line coverage below {minimum:g}% ({len(failures)} of {checked} files):",
            file=sys.stderr,
        )
        for filename, percent, covered, count in sorted(failures):
            print(
                f"  {filename}: {percent:.2f}% ({covered}/{count} lines)",
                file=sys.stderr,
            )
        return 1

    print(f"per-file line coverage >= {minimum:g}% for {checked} files")
    return 0


def relative_src_path(filename: str) -> str:
    marker = "/src/"
    if marker in filename:
        return "src/" + filename.rsplit(marker, 1)[1]
    return filename


if __name__ == "__main__":
    raise SystemExit(main())
