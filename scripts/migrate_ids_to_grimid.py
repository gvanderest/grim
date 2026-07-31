#!/usr/bin/env python3
"""Migrate persisted account/character ids from UUID to base62 Grim IDs.

Grim IDs are a base62 (`A-Za-z0-9`) nanoid of length 12 — see
docs/adr/0001-area-identity-and-instancing.md.

Scope: only accounts and characters are persisted with ids today (areas/rooms
are code-seeded, and `last_room` already uses slugs, not ids). The ids
cross-reference each other, so this remaps every id ONCE and rewrites all
references coherently:

    account.id            <-> character.account_id
    account.characters[]  <-> character.id

`last_room` (area/room slugs) is left untouched. Account files are named
`<id>.json`, so they are renamed to the new id; character files are named
`<name>.json` and keep their name.

PREREQUISITE: run this only together with / after the `Uuid -> GrimId` code
change. The current binary types these ids as `Uuid` and will fail to load
base62 ids. Nothing here is reversible — back up `data/` first if you care.

Usage:
    python3 scripts/migrate_ids_to_grimid.py [DATA_DIR]   # default: ./data
    python3 scripts/migrate_ids_to_grimid.py --dry-run [DATA_DIR]
"""

from __future__ import annotations

import argparse
import json
import secrets
import string
import sys
from pathlib import Path

ALPHABET = string.ascii_letters + string.digits  # base62: A-Za-z0-9
GRIM_ID_LEN = 12

# A value looks already-migrated if it is exactly a base62 string of the target
# length (a UUID contains '-', so it never matches).
def is_grim_id(value: str) -> bool:
    return (
        isinstance(value, str)
        and len(value) == GRIM_ID_LEN
        and all(c in ALPHABET for c in value)
    )


def new_grim_id(existing: set[str]) -> str:
    while True:
        candidate = "".join(secrets.choice(ALPHABET) for _ in range(GRIM_ID_LEN))
        if candidate not in existing:
            existing.add(candidate)
            return candidate


def load_json(path: Path) -> dict:
    with path.open() as f:
        return json.load(f)


def dump_json(path: Path, data: dict) -> None:
    with path.open("w") as f:
        json.dump(data, f, indent=2)
        f.write("\n")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("data_dir", nargs="?", default="data")
    ap.add_argument("--dry-run", action="store_true", help="print changes, write nothing")
    args = ap.parse_args()

    data = Path(args.data_dir)
    accounts_dir = data / "accounts"
    characters_dir = data / "characters"
    if not accounts_dir.is_dir() and not characters_dir.is_dir():
        print(f"no accounts/ or characters/ under {data}", file=sys.stderr)
        return 1

    account_files = sorted(accounts_dir.glob("*.json")) if accounts_dir.is_dir() else []
    character_files = (
        sorted(characters_dir.glob("*.json")) if characters_dir.is_dir() else []
    )

    accounts = {p: load_json(p) for p in account_files}
    characters = {p: load_json(p) for p in character_files}

    # ── Build the old-id -> new-id map over every canonical id. The canonical
    # ids are account.id and character.id; account_id / characters[] are refs. ──
    used: set[str] = set()
    # Seed `used` with any ids already migrated so we never collide with them.
    for obj in list(accounts.values()) + list(characters.values()):
        if is_grim_id(obj.get("id", "")):
            used.add(obj["id"])

    id_map: dict[str, str] = {}

    def remap(old: str) -> str:
        if is_grim_id(old):
            return old  # already migrated, leave as-is
        if old not in id_map:
            id_map[old] = new_grim_id(used)
        return id_map[old]

    for obj in accounts.values():
        remap(obj["id"])
    for obj in characters.values():
        remap(obj["id"])

    # ── Apply, rewriting refs. A ref to an id we never saw as canonical is
    # dangling; warn and leave it untouched rather than invent a mapping. ──
    def resolve_ref(ref: str, where: str) -> str:
        if is_grim_id(ref):
            return ref
        if ref in id_map:
            return id_map[ref]
        print(f"  WARN dangling reference {ref} in {where} — left unchanged")
        return ref

    for obj in accounts.values():
        obj["id"] = remap(obj["id"])
        obj["characters"] = [
            resolve_ref(c, "account.characters") for c in obj.get("characters", [])
        ]
    for obj in characters.values():
        obj["id"] = remap(obj["id"])
        obj["account_id"] = resolve_ref(obj.get("account_id", ""), "character.account_id")

    # ── Write. Accounts are renamed to <new-id>.json; characters keep <name>.json. ──
    print(f"remapped {len(id_map)} id(s)")
    for old, new in id_map.items():
        print(f"  {old} -> {new}")

    if args.dry_run:
        print("dry-run: no files written")
        return 0

    for path, obj in accounts.items():
        target = path.with_name(f"{obj['id']}.json")
        dump_json(target, obj)
        if target != path:
            path.unlink()
            print(f"  account {path.name} -> {target.name}")
    for path, obj in characters.items():
        dump_json(path, obj)

    print("done")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
