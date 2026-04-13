#!/usr/bin/env python3

import argparse
import datetime as dt
import pathlib
import re
import sys


VERSION_RE = re.compile(r'(^version\s*=\s*")([^"]+)(")', re.MULTILINE)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Bump workspace version using YYMM.D.BUILD")
    parser.add_argument(
        "--manifest-path",
        default="Cargo.toml",
        help="Path to the workspace Cargo.toml",
    )
    parser.add_argument(
        "--date",
        help="Override date in YYYY-MM-DD format for deterministic bumps",
    )
    parser.add_argument(
        "--print-current",
        action="store_true",
        help="Print the current workspace version without modifying the file",
    )
    return parser.parse_args()


def load_today(raw: str | None) -> dt.date:
    if raw is None:
        return dt.date.today()
    return dt.date.fromisoformat(raw)


def parse_version(raw: str) -> tuple[int, int, int]:
    parts = raw.split(".")
    if len(parts) != 3:
        raise ValueError(f"unsupported version format: {raw}")
    yymm = int(parts[0])
    day = int(parts[1])
    build = int(parts[2])
    return yymm, day, build


def next_version(current: str, today: dt.date) -> str:
    yymm = int(f"{today.year % 100:02d}{today.month:02d}")
    _, day, build = parse_version(current)

    if current.startswith(f"{yymm}.{today.day}."):
        return f"{yymm}.{today.day}.{build + 1}"
    return f"{yymm}.{today.day}.1"


def main() -> int:
    args = parse_args()
    manifest_path = pathlib.Path(args.manifest_path)
    raw = manifest_path.read_text(encoding="utf-8")
    match = VERSION_RE.search(raw)
    if match is None:
        raise SystemExit(f"failed to find version entry in {manifest_path}")

    current = match.group(2)
    if args.print_current:
        print(current)
        return 0

    today = load_today(args.date)
    updated = next_version(current, today)
    rewritten = VERSION_RE.sub(rf'\g<1>{updated}\g<3>', raw, count=1)
    manifest_path.write_text(rewritten, encoding="utf-8")
    print(updated)
    return 0


if __name__ == "__main__":
    sys.exit(main())
