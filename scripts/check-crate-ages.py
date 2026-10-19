#!/usr/bin/env python3
"""Supply-chain cooldown check: fail if any crates.io package in Cargo.lock is too new.

Usage: python3 scripts/check-crate-ages.py [--days 14] [--lock Cargo.lock] [--only-changed REF]
                                          [--allow-git URL ...] [--cache FILE] [--release]

Dates each locked crates.io version by its file on static.crates.io first (a CDN, no rate
limit; never earlier than the publish date), and asks the crates.io API (one request per crate,
one per second, per its crawler policy, retrying when asked to slow down) only about versions
that look young. Lists any locked version younger than the cooldown, with the newest compatible
version old enough to pin to. A package from
anywhere else (a git repository, another registry) has no publish date to check, so it
fails the check too: fail closed. The one exception is a git repository passed with
--allow-git, and only when the lock pins it to an exact revision (reviewed once, by rev).

An exact NAME VERSION listed in scripts/crate-age-exceptions.txt (the same format as Method
Reader's), with its reason and age-out date, may be younger than the cooldown, so main stays
green while a version that cannot be pinned back ages out, or a young version that fixes a
security advisory (its reason names the RUSTSEC id) can ship. --release honours only the
security lines: the release workflow runs it so, and no release is tagged while an age-out
line's version is still young. A yanked version always fails.

This copy converges on the suite's shared crate-age script (SUITE-STANDARDS.md §6, with an
allow-list file) once method-libs carries it. With --only-changed REF, only packages added or bumped
since that git ref are checked. Stdlib only, so it runs anywhere Python 3 does.
"""

import argparse
import re
import datetime as dt
import email.utils
import json
import pathlib
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request

UA = "method-storyteller cooldown check (https://github.com/method-production-suite/method-storyteller)"


CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
EXCEPTIONS = pathlib.Path(__file__).with_name("crate-age-exceptions.txt")


def excepted(security_only=False):
    """{(name, version)} allowed under the cooldown, each listed with a reason. With
    security_only, only the lines whose reason names a RUSTSEC advisory (a young fix that
    cannot wait), not the ones waiting to age out."""
    if not EXCEPTIONS.exists():
        return set()
    out = set()
    for line in EXCEPTIONS.read_text().splitlines():
        entry, _, reason = line.partition("#")
        words = entry.split()
        if len(words) == 2 and (not security_only or "RUSTSEC-" in reason):
            out.add((words[0], words[1]))
    return out
PINNED_GIT = re.compile(r"^git\+(https://[^?#]+)\?rev=([0-9a-f]{40})#\2$")


def external_packages(lock_text):
    """(name, version, source) of every locked package that isn't a workspace member."""
    lock = tomllib.loads(lock_text)
    return {
        (p["name"], p["version"], p["source"]) for p in lock.get("package", []) if p.get("source")
    }


def versions(name):
    """{version: (published, yanked)} for every version of a crate. Retries a 429 or a 5xx,
    waiting as long as the server asks (or doubling from 10 s, up to 5 minutes), nine times."""
    req = urllib.request.Request(f"https://crates.io/api/v1/crates/{name}", headers={"User-Agent": UA})
    wait = 10
    for attempt in range(10):
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                data = json.load(r)
            break
        except urllib.error.HTTPError as e:
            if attempt == 9 or (e.code != 429 and e.code < 500):
                raise
            asked = e.headers.get("Retry-After", "")
            time.sleep(int(asked) if asked.isdigit() else wait)
            wait = min(wait * 2, 300)
    return {
        v["num"]: (dt.datetime.fromisoformat(v["created_at"].replace("Z", "+00:00")), v["yanked"])
        for v in data["versions"]
    }


def stored_at(name, version):
    """When static.crates.io last wrote the .crate file (its Last-Modified), or None. Never
    earlier than the publish date (files are sometimes rewritten later, never before), so a
    crate this shows as old enough is old enough, and the API rate limit is spent only on the
    rest."""
    req = urllib.request.Request(
        f"https://static.crates.io/crates/{name}/{name}-{version}.crate", method="HEAD", headers={"User-Agent": UA}
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            stamp = r.headers.get("Last-Modified")
    except urllib.error.URLError:
        return None
    return email.utils.parsedate_to_datetime(stamp) if stamp else None


def index_path(name):
    """A crate's path in the sparse index (index.crates.io), by Cargo's naming rule."""
    lower = name.lower()
    if len(lower) <= 2:
        return f"{len(lower)}/{lower}"
    if len(lower) == 3:
        return f"3/{lower[0]}/{lower}"
    return f"{lower[:2]}/{lower[2:4]}/{lower}"


def yanked_in_index(name, version):
    """Whether the sparse index marks this version yanked (a CDN, no rate limit). None when the
    index cannot be read."""
    req = urllib.request.Request(f"https://index.crates.io/{index_path(name)}", headers={"User-Agent": UA})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            lines = r.read().decode().splitlines()
    except urllib.error.URLError:
        return None
    for line in lines:
        entry = json.loads(line)
        if entry.get("vers") == version:
            return bool(entry.get("yanked"))
    return None


def semver(version):
    """(major, minor, patch) of a release (build metadata such as `+spec-1.1.0` ignored), or
    None for a pre-release."""
    m = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:\+[0-9A-Za-z.-]+)?", version)
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
    ap.add_argument("--cache", metavar="FILE", help="keep fetched version lists here, so a local run can resume")
    ap.add_argument(
        "--release",
        action="store_true",
        help="for a release: honour only the exceptions that name a RUSTSEC advisory, not the age-outs",
    )
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
    if args.cache:
        try:
            with open(args.cache) as f:
                known = {
                    name: {v: (dt.datetime.fromisoformat(when), yanked) for v, (when, yanked) in vs.items()}
                    for name, vs in json.load(f).items()
                }
        except FileNotFoundError:
            pass
    pins = []
    allowed_young = excepted(security_only=args.release)
    still_young = set()
    for i, (name, version) in enumerate(sorted(pkgs)):
        stored = stored_at(name, version) if name not in known else None
        yanked = yanked_in_index(name, version) if stored and stored <= cutoff else None
        if stored and stored <= cutoff and yanked is not None:
            # Old enough by its file date alone; the index says whether it was yanked.
            flag = "  YANKED" if yanked else ""
            if yanked:
                bad.append(f"{name} {version} (yanked)")
            print(f"{name:40} {version:16} {(now - stored).days:5}+ days{flag}")
            continue
        if name not in known or version not in known[name]:
            time.sleep(1)
            known[name] = versions(name)
            if args.cache:
                with open(args.cache, "w") as f:
                    json.dump({n: {v: (w.isoformat(), y) for v, (w, y) in vs.items()} for n, vs in known.items()}, f)
        when, yanked = known[name][version]
        age = (now - when).days
        flag = ""
        if age < args.days and (name, version) in allowed_young:
            still_young.add((name, version))
            flag = "  young, listed as an exception (scripts/crate-age-exceptions.txt)"
        elif age < args.days:
            target = pin_to(known[name], version, cutoff)
            flag = f"  TOO NEW (< {args.days} days)" + (f", pin to {target}" if target else ", no older compatible version")
            bad.append(f"{name} {version}")
            if target:
                pins.append(f"cargo update -p {name}@{version} --precise {target}")
        if yanked:
            flag += "  YANKED"
            bad.append(f"{name} {version} (yanked)")
        print(f"{name:40} {version:16} {age:5} days{flag}")

    # A listed version that is old enough now, or no longer locked, is only a reminder. With
    # --only-changed the run sees only the changed entries, so it cannot tell; it says nothing.
    for name, version in sorted(allowed_young - still_young if not args.only_changed else set()):
        where = "now 14 days old" if (name, version) in pkgs else "not in the lock"
        print(f"note: {name} {version} is {where}; remove its line from {EXCEPTIONS.name}")
    print(f"\nchecked {len(pkgs)} crates.io crates; {len(bad)} problem(s)")
    if bad:
        print("downgrade with:")
        for pin in pins:
            print(f"  {pin}")
        sys.exit(1)


if __name__ == "__main__":
    main()
