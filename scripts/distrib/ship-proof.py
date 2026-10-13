#!/usr/bin/env python3
"""Generate the closed C31 proof from four actual local distribution archives.

Hashes and member inventories prove local byte consistency and nonshipment.
They do not attest native architecture, signing, or release provenance.
"""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import tarfile
import time

TARGETS = ('aarch64-apple-darwin', 'x86_64-apple-darwin',
           'aarch64-unknown-linux-musl', 'x86_64-unknown-linux-musl')
ARCHIVES = tuple('thruline-cli-' + target + '.tar.gz' for target in TARGETS)
FAMILIES = ('docs/**', 'CLAUDE.md', 'LIVE-HANDOFF.md', 'REPORT.md',
            'ui/src/hq/screens/*/DATA.md')
MAX_BYTES = 1024 ** 3
MAX_LIST_BYTES = 4 * 1024 ** 2


class Refusal(ValueError):
    pass


def ordinary_path(path, directory=False):
    """Check ancestors before resolving; resolving first would hide symlinks."""
    path = Path(path)
    if not path.is_absolute() or any(part in ('.', '..') for part in str(path).split('/')):
        raise Refusal('path must be absolute with ordinary components')
    current = Path(path.anchor)
    for index, component in enumerate(path.parts[1:]):
        current /= component
        info = current.lstat()
        leaf = index == len(path.parts) - 2
        want_dir = directory or not leaf
        if stat.S_ISLNK(info.st_mode) or (want_dir and not stat.S_ISDIR(info.st_mode)):
            raise Refusal('symlink or nonregular directory: ' + str(current))
        if leaf and not directory and (not stat.S_ISREG(info.st_mode) or info.st_nlink != 1):
            raise Refusal('archive/proof must be a regular unaliased file')
    return path


def identity(path):
    info = ordinary_path(path).stat()
    if info.st_size > MAX_BYTES:
        raise Refusal('archive exceeds byte bound')
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def digest(path, deadline):
    before = identity(path)
    result = hashlib.sha256()
    amount = 0
    with path.open('rb') as source:
        while True:
            if time.monotonic() >= deadline:
                raise Refusal('archive verification exceeded time bound')
            chunk = source.read(64 * 1024)
            if not chunk:
                break
            amount += len(chunk)
            if amount > MAX_BYTES:
                raise Refusal('archive exceeds byte bound')
            result.update(chunk)
    if amount != before[2] or identity(path) != before:
        raise Refusal('archive changed while hashing')
    return result.hexdigest(), before


class BoundedReader:
    def __init__(self, source, deadline):
        self.source, self.deadline, self.amount = source, deadline, 0

    def read(self, amount):
        if time.monotonic() >= self.deadline:
            raise Refusal('archive listing exceeded time bound')
        data = self.source.read(amount)
        self.amount += len(data)
        if self.amount > MAX_BYTES:
            raise Refusal('uncompressed archive exceeds byte bound')
        return data


def family_matches(name):
    parts = name.split('/')[1:]
    matches = []
    if 'docs' in parts:
        matches.append('docs/**')
    for family in FAMILIES[1:4]:
        if parts[-1:] == [family]:
            matches.append(family)
    if len(parts) >= 6 and parts[-6:-2] == ['ui', 'src', 'hq', 'screens'] and parts[-1] == 'DATA.md':
        matches.append(FAMILIES[-1])
    return matches


def inventory(path, deadline):
    expected_root = path.name[:-7]
    seen, names, families = set(), [], {name: [] for name in FAMILIES}
    listing_bytes, executable = 0, False
    with path.open('rb') as raw, gzip.GzipFile(fileobj=raw) as compressed:
        with tarfile.open(fileobj=BoundedReader(compressed, deadline), mode='r|') as archive:
            for member in archive:
                name = member.name.rstrip('/')
                listing_bytes += len(name.encode('utf-8', errors='strict')) + 1
                if listing_bytes > MAX_LIST_BYTES:
                    raise Refusal('archive listing exceeds byte bound')
                parts = name.split('/')
                if (not name or any(not part or part in ('.', '..') for part in parts)
                        or any(ord(char) < 32 or ord(char) == 127 for char in name)
                        or '\\' in name or parts[0] != expected_root or name in seen):
                    raise Refusal('unsafe, duplicate, or wrong-root archive member')
                seen.add(name); names.append(name)
                if not (member.isfile() or member.isdir()):
                    raise Refusal('archive contains a link or special member')
                for family in family_matches(name):
                    families[family].append(name)
                if any(families.values()):
                    raise Refusal('A1 archive family is shipped: ' + name)
                if member.isdir():
                    if name != expected_root:
                        raise Refusal('unexpected archive directory')
                elif len(parts) != 2 or parts[1] not in ('thruline', 'README.md', 'LICENSE.md', 'THIRD_PARTY_NOTICES.md', 'CHANGELOG.md'):
                    raise Refusal('unexpected public package member')
                elif parts[1] == 'thruline':
                    if member.size <= 0 or not member.mode & 0o111:
                        raise Refusal('archive executable is empty or not executable')
                    executable = True
            # Drain the gzip stream to validate its trailer and enforce the full
            # uncompressed bound even when tar has already reached its end.
            while archive.fileobj.read(64 * 1024):
                pass
    if not executable:
        raise Refusal('archive has no nonempty executable')
    listing = '\n'.join(names) + '\n'
    return {'file_name': path.name, 'member_count': len(names),
            'member_names_sha256': hashlib.sha256(listing.encode()).hexdigest()}, families


def exact_archive_set(directory):
    ordinary_path(directory, directory=True)
    actual = {item.name for item in directory.iterdir() if item.name.endswith('.tar.gz')}
    if actual != set(ARCHIVES):
        raise Refusal('distribution must contain exactly the four named platform archives')
    return [ordinary_path(directory / name) for name in ARCHIVES]


def checked_head(repo, supplied=None):
    ordinary_path(repo, directory=True)
    head = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', '--verify', 'HEAD'],
                                   text=True, timeout=10).strip()
    if len(head) not in (40, 64) or any(char not in '0123456789abcdef' for char in head):
        raise Refusal('repository HEAD is not one exact Git object id')
    if supplied is not None and supplied != head:
        raise Refusal('checked commit differs from repository HEAD')
    return head


def write_new_json(path, value):
    data = (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()
    if len(data) > 64 * 1024:
        raise Refusal('proof/receipt exceeds byte bound')
    with path.open('xb') as output:
        os.chmod(path, 0o600)
        output.write(data)


def generate(repo, dist, checked_commit=None):
    head, deadline = checked_head(repo, checked_commit), time.monotonic() + 30
    proof_path = dist / 'decisions-ship-proof.json'
    if proof_path.exists() or proof_path.is_symlink():
        raise Refusal('existing ship proof must not be overwritten')
    hashes, inventories, families = [], [], {name: [] for name in FAMILIES}
    for path in exact_archive_set(dist):
        sha, before = digest(path, deadline)
        item, found = inventory(path, deadline)
        after_sha, after = digest(path, deadline)
        if sha != after_sha or before != after:
            raise Refusal('archive changed during member inspection')
        hashes.append({'file_name': path.name, 'sha256': sha})
        inventories.append(item)
        for family, paths in found.items():
            families[family].extend(paths)
    if checked_head(repo) != head:
        raise Refusal('repository HEAD changed during dry run')
    proof = {'schema_version': 1, 'release_version': 'v0.2.0', 'checked_commit': head,
             'source': 'private-release-dry-run', 'archives': hashes}
    write_new_json(proof_path, proof)
    return proof, inventories, families


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--dist', type=Path, required=True)
    parser.add_argument('--checked-commit')
    args = parser.parse_args()
    try:
        proof, inventories, families = generate(args.repo, args.dist, args.checked_commit)
        print(json.dumps({'proof': str(args.dist / 'decisions-ship-proof.json'),
                          'checked_commit': proof['checked_commit'],
                          'archive_inventory': inventories, 'family_members': families,
                          'native_platform_build_proved': False}))
    except (OSError, ValueError, tarfile.TarError, subprocess.SubprocessError) as error:
        parser.exit(1, 'ship-proof refused: ' + str(error) + '\n')


if __name__ == '__main__':
    main()
