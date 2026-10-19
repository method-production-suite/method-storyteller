#!/usr/bin/env python3
"""Supply-chain cooldown check: fail if any crates.io package in Cargo.lock is too new.

Usage: python3 scripts/check-crate-ages.py [--days 14] [--lock Cargo.lock] [--only-changed REF]
                                          [--allow-git URL ...]

Looks up each locked crates.io crate's versions (one request per crate, one per second, per
crates.io's crawler policy, retrying when asked to slow down) and lists any locked version
younger than the cooldown, with the newest compatible version old enough to pin to. A package from
anywhere else (a git repository, another registry) has no publish date to check, so it
fails the check too: fail closed. The one exception is a git repository passed with
--allow-git, and only when the lock pins it to an exact revision (reviewed once, by rev).

This copy converges on the suite's shared crate-age script (SUITE-STANDARDS.md §6, with an
allow-list file) once method-libs carries it. With --only-changed REF, only packages added or bumped
since that git ref are checked. Stdlib only, so it runs anywhere Python 3 does.
"""

import argparse
import re
import datetime as dt
import json
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request

UA = "method-storyteller cooldown check (https://github.com/method-production-suite/method-storyteller)"


CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
PINNED_GIT = re.compile(r"^git\+(https://[^?#]+)\?rev=([0-9a-f]{40})#\2$")


def external_packages(lock_text):
    """(name, version, source) of every locked package that isn't a workspace member."""
    lock = tomllib.loads(lock_text)
    return {
        (p["name"], p["version"], p["source"]) for p in lock.get("package", []) if p.get("source")
    }


def versions(name):
    """{version: (published, yanked)} for every version of a crate. Retries a 429 or a 5xx,
    waiting as long as the server asks (or doubling from 5 s), up to six times."""
    req = urllib.request.Request(f"https://crates.io/api/v1/crates/{name}", headers={"User-Agent": UA})
    wait = 5
    for attempt in range(7):
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                data = json.load(r)
            break
        except urllib.error.HTTPError as e:
            if attempt == 6 or (e.code != 429 and e.code < 500):
                raise
            asked = e.headers.get("Retry-After", "")
            time.sleep(int(asked) if asked.isdigit() else wait)
            wait = min(wait * 2, 120)
    return {
        v["num"]: (dt.datetime.fromisoformat(v["created_at"].replace("Z", "+00:00")), v["yanked"])
        for v in data["versions"]
    }


def semver(version):
    """(major, minor, patch) of a plain release, or None for a pre-release or build."""
    m = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", version)
    return tuple(int(x) for x in m.groups()) if m else None


def compatible(a, b):
    """Cargo's caret rule: the same leftmost non-zero component."""
    if a[0] != b[0]:
        return False
    return a[0] != 0 or (a[1] == b[1] and (a[1] != 0 or a[2] == b[2]))


def pin_to(all_versions, locked, cutoff):
    """The newest unyanked version, compatible with and not newer than the locked one, published
    before the cutoff, or None."""
    want = semver(locked)
    if not want:
        return None
    ok = [
        (semver(v), v)
        for v, (when, yanked) in all_versions.items()
        if semver(v) and not yanked and when <= cutoff and compatible(semver(v), want) and semver(v) <= want
    ]
    return max(ok)[1] if ok else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--days", type=int, default=14)
    ap.add_argument("--lock", default="Cargo.lock")
    ap.add_argument("--only-changed", metavar="REF")
    ap.add_argument("--allow-git", action="append", default=[], metavar="URL")
    args = ap.parse_args()

    with open(args.lock) as f:
        pkgs = external_packages(f.read())
    if args.only_changed:
        old = subprocess.run(
            ["git", "show", f"{args.only_changed}:{args.lock}"], capture_output=True, text=True
        )
        if old.returncode == 0:
            pkgs -= external_packages(old.stdout)

    bad = []

    def reviewed(source):
        # git+URL?rev=SHA#SHA, the same full 40-hex revision twice: a pinned revision of a
        # repository on the allow list (the pattern Method Reader uses).
        match = PINNED_GIT.match(source)
        return bool(match) and match.group(1) in args.allow_git

    for name, version, source in sorted(p for p in pkgs if p[2] != CRATES_IO and reviewed(p[2])):
        print(f"{name:40} {version:16}  pinned git revision, reviewed ({source.split('?')[0]})")
    pkgs = {p for p in pkgs if p[2] == CRATES_IO or not reviewed(p[2])}
    for name, version, source in sorted(p for p in pkgs if p[2] != CRATES_IO):
        print(f"{name:40} {version:16}  NOT FROM CRATES.IO ({source}): no publish date to check")
        bad.append(f"{name} {version} ({source})")
    pkgs = {(name, version) for name, version, source in pkgs if source == CRATES_IO}

    now = dt.datetime.now(dt.timezone.utc)
    cutoff = now - dt.timedelta(days=args.days)
    known = {}
    pins = []
    for i, (name, version) in enumerate(sorted(pkgs)):
        if name not in known:
            if known:
                time.sleep(1)
            known[name] = versions(name)
        when, yanked = known[name][version]
        age = (now - when).days
        flag = ""
        if age < args.days:
            target = pin_to(known[name], version, cutoff)
            flag = f"  TOO NEW (< {args.days} days)" + (f", pin to {target}" if target else ", no older compatible version")
            bad.append(f"{name} {version}")
            if target:
                pins.append(f"cargo update -p {name}@{version} --precise {target}")
        if yanked:
            flag += "  YANKED"
            bad.append(f"{name} {version} (yanked)")
        print(f"{name:40} {version:16} {age:5} days{flag}")

    print(f"\nchecked {len(pkgs)} crates.io crates; {len(bad)} problem(s)")
    if bad:
        print("downgrade with:")
        for pin in pins:
            print(f"  {pin}")
        sys.exit(1)


if __name__ == "__main__":
    main()
