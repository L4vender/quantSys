#!/usr/bin/env python3
"""Idempotently create Redpanda/Kafka topics from scripts/topic-init/topics.toml."""

from __future__ import annotations

import argparse
import shlex
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_TOPICS = ROOT / "scripts" / "topic-init" / "topics.toml"
DEFAULT_RPK = (
    "docker compose -f deploy/docker-compose/docker-compose.yml "
    "--profile local exec -T redpanda rpk -X brokers=localhost:9092"
)


def parse_topics(path: Path) -> list[dict[str, object]]:
    topics: list[dict[str, object]] = []
    current: dict[str, object] | None = None
    for raw_line in path.read_text().splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line == "[[topics]]":
            if current:
                topics.append(current)
            current = {}
            continue
        if current is None or "=" not in line:
            continue
        key, value = [part.strip() for part in line.split("=", 1)]
        current[key] = parse_value(value)
    if current:
        topics.append(current)
    return topics


def parse_value(value: str) -> object:
    if value.startswith('"') and value.endswith('"'):
        return value[1:-1]
    if value.startswith("[") and value.endswith("]"):
        inner = value[1:-1].strip()
        if not inner:
            return []
        return [item.strip().strip('"') for item in inner.split(",")]
    try:
        return int(value)
    except ValueError:
        return value


def retention_ms(days: int) -> int:
    return days * 24 * 60 * 60 * 1000


def build_command(rpk: str, topic: dict[str, object]) -> list[str]:
    return shlex.split(rpk) + [
        "topic",
        "create",
        str(topic["name"]),
        "--partitions",
        str(topic["partitions"]),
        "--replicas",
        str(topic["replicas"]),
        "--topic-config",
        f"retention.ms={retention_ms(int(topic['retention_days']))}",
    ]


def existing_topics(rpk: str) -> set[str]:
    command = shlex.split(rpk) + ["topic", "list"]
    output = subprocess.check_output(command, cwd=ROOT, text=True)
    names: set[str] = set()
    for line in output.splitlines()[1:]:
        columns = line.split()
        if columns:
            names.add(columns[0])
    return names


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--topics", type=Path, default=DEFAULT_TOPICS)
    parser.add_argument("--rpk", default=DEFAULT_RPK)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    topics = parse_topics(args.topics)
    if not topics:
        raise SystemExit(f"no topics found in {args.topics}")

    existing = set() if args.dry_run else existing_topics(args.rpk)
    for topic in topics:
        name = str(topic["name"])
        if name in existing:
            print(f"ok - topic {name} exists")
            continue
        command = build_command(args.rpk, topic)
        if args.dry_run:
            print(" ".join(shlex.quote(part) for part in command))
            continue
        subprocess.run(command, cwd=ROOT, check=True)
        print(f"ok - topic {name} created")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
