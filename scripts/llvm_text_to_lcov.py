#!/usr/bin/env python3
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path


SOURCE_LINE_RE = re.compile(r"^\s*(\d+)\|\s*([0-9]+)\|")
BRANCH_RE = re.compile(r"^\s*\|\s+Branch \((\d+):(\d+)\): \[(.*)\]")
BRANCH_PART_RE = re.compile(r"[^:,\]]+:\s*([0-9]+)")
TEST_CFG_RE = re.compile(r"^\s*#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]")


@dataclass
class FileCoverage:
    path: str
    lines: dict[int, int] = field(default_factory=dict)
    branches: dict[tuple[int, int, int], int] = field(default_factory=dict)
    test_start_line: int | None = None

    def add_branch_group(self, line: int, column: int, counts: list[int]) -> None:
        if not counts:
            return
        for index, count in enumerate(counts):
            key = (line, column, index)
            self.branches[key] = self.branches.get(key, 0) + count

    def contains_production_line(self, line_number: int) -> bool:
        return self.test_start_line is None or line_number < self.test_start_line


def parse_llvm_text(path: Path) -> list[FileCoverage]:
    files: list[FileCoverage] = []
    current: FileCoverage | None = None

    with path.open("r", encoding="utf-8") as report:
        for raw_line in report:
            line = raw_line.rstrip("\n")
            if line.endswith(":") and not line.startswith(" ") and line[:-1].endswith(".rs"):
                source_path = line[:-1]
                current = FileCoverage(source_path, test_start_line=find_test_start_line(Path(source_path)))
                files.append(current)
                continue
            if current is None:
                continue

            source_match = SOURCE_LINE_RE.match(line)
            if source_match:
                line_number = int(source_match.group(1))
                if not current.contains_production_line(line_number):
                    continue
                hit_count = int(source_match.group(2))
                current.lines[line_number] = hit_count
                continue

            branch_match = BRANCH_RE.match(line)
            if branch_match and "Folded - Ignored" not in branch_match.group(3):
                line_number = int(branch_match.group(1))
                if not current.contains_production_line(line_number):
                    continue
                counts = [int(match.group(1)) for match in BRANCH_PART_RE.finditer(branch_match.group(3))]
                current.add_branch_group(
                    line_number,
                    int(branch_match.group(2)),
                    counts,
                )

    return files


def find_test_start_line(path: Path) -> int | None:
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError:
        return None

    for index, line in enumerate(lines, start=1):
        if TEST_CFG_RE.match(line):
            return index
    return None


def write_lcov(files: list[FileCoverage], path: Path) -> None:
    with path.open("w", encoding="utf-8") as output:
        for file_coverage in files:
            if not file_coverage.lines and not file_coverage.branches:
                continue
            output.write(f"SF:{file_coverage.path}\n")

            for line_number in sorted(file_coverage.lines):
                output.write(f"DA:{line_number},{file_coverage.lines[line_number]}\n")
            if file_coverage.lines:
                found_lines = len(file_coverage.lines)
                hit_lines = sum(1 for count in file_coverage.lines.values() if count > 0)
                output.write(f"LF:{found_lines}\n")
                output.write(f"LH:{hit_lines}\n")

            for (line_number, block, branch), count in sorted(file_coverage.branches.items()):
                output.write(f"BRDA:{line_number},{block},{branch},{count}\n")
            if file_coverage.branches:
                found_branches = len(file_coverage.branches)
                hit_branches = sum(1 for count in file_coverage.branches.values() if count > 0)
                output.write(f"BRF:{found_branches}\n")
                output.write(f"BRH:{hit_branches}\n")

            output.write("end_of_record\n")


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: scripts/llvm_text_to_lcov.py <llvm-text-report> <lcov-output>", file=sys.stderr)
        return 2

    input_path = Path(sys.argv[1])
    output_path = Path(sys.argv[2])
    files = parse_llvm_text(input_path)
    write_lcov(files, output_path)
    print(f"LCOV report generated from LLVM text report: {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
