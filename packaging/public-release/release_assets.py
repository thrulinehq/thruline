#!/usr/bin/env python3
"""Stage and verify the five assets allowed in a public ThruLine release.

The draft holds exactly the four CLI archives and SHA256SUMS, with those names
(the website's installer downloads by them). LICENSE.md and
THIRD_PARTY_NOTICES.md travel inside each archive, not beside it.
"""

import argparse
import hashlib
import re
import shutil
import stat
import tarfile
from pathlib import Path


TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-musl",
)
ARCHIVES = tuple(f"thruline-cli-{target}.tar.gz" for target in TARGETS)
ASSETS = set(ARCHIVES) | {"SHA256SUMS"}
SUM_LINE = re.compile(r"([0-9a-f]{64}) [ *](\S+)")
SIDECAR_LINE = re.compile(r"([0-9a-f]{64})(?: [ *](\S+))?")


def fail(message: str) -> None:
    raise SystemExit(f"public release assets: {message}")


def regular_file(path: Path) -> None:
    try:
        mode = path.lstat().st_mode
    except FileNotFoundError:
        fail(f"missing {path}")
    if not stat.S_ISREG(mode):
        fail(f"not a regular file: {path}")


def sums(path: Path) -> dict[str, str]:
    regular_file(path)
    result = {}
    for line in path.read_text(encoding="ascii").splitlines():
        match = SUM_LINE.fullmatch(line)
        if not match:
            fail(f"invalid checksum line in {path.name}")
        digest, name = match.groups()
        if name not in ARCHIVES or name in result:
            fail(f"unexpected or duplicate checksum entry: {name}")
        result[name] = digest
    if set(result) != set(ARCHIVES):
        fail(f"{path.name} must list exactly the four CLI archives")
    return result


def verify_archive(path: Path, target: str) -> None:
    root = f"thruline-cli-{target}"
    allowed = {f"{root}/{name}" for name in ("thruline", "README.md", "LICENSE.md", "THIRD_PARTY_NOTICES.md", "CHANGELOG.md")}
    seen = set()
    try:
        with tarfile.open(path, "r:gz") as archive:
            for member in archive:
                if member.name == root and member.isdir():
                    continue
                if member.name not in allowed or not member.isfile() or member.name in seen:
                    fail(f"unexpected archive member in {path.name}: {member.name}")
                seen.add(member.name)
                if member.name == f"{root}/thruline" and (member.size == 0 or member.mode & 0o111 == 0):
                    fail(f"missing executable permissions or empty binary in {path.name}")
    except (tarfile.TarError, OSError) as error:
        fail(f"cannot inspect {path.name}: {error}")
    # The licence and the third-party notices travel with every binary.
    for required in ("thruline", "LICENSE.md", "THIRD_PARTY_NOTICES.md"):
        if f"{root}/{required}" not in seen:
            fail(f"archive does not contain {root}/{required}")


def archived_bytes(path: Path, target: str, name: str) -> bytes:
    """The bytes of one file inside a CLI archive."""
    try:
        with tarfile.open(path, "r:gz") as archive:
            member = archive.extractfile(f"thruline-cli-{target}/{name}")
            if member is None:
                fail(f"{path.name} has no {name}")
            return member.read()
    except (tarfile.TarError, OSError, KeyError) as error:
        fail(f"cannot read {name} in {path.name}: {error}")


def verify(directory: Path, checksum_name: str = "SHA256SUMS") -> dict[str, str]:
    expected = set(ARCHIVES) | {checksum_name}
    actual = {path.name for path in directory.iterdir()}
    if actual != expected:
        fail(f"asset set differs; missing={sorted(expected - actual)}, extra={sorted(actual - expected)}")
    checksums = sums(directory / checksum_name)
    for target, name in zip(TARGETS, ARCHIVES):
        path = directory / name
        regular_file(path)
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        if digest != checksums[name]:
            fail(f"checksum mismatch: {name}")
        verify_archive(path, target)
    return checksums


def stage(artifacts: Path, source: Path, output: Path) -> None:
    if not artifacts.is_dir() or not source.is_dir():
        fail("artifact and source directories must exist")
    expected_inputs = set(ARCHIVES) | {f"{name}.sha256" for name in ARCHIVES}
    actual_inputs = {path.name for path in artifacts.iterdir()}
    if actual_inputs != expected_inputs:
        fail(
            "build input set differs; "
            f"missing={sorted(expected_inputs - actual_inputs)}, "
            f"extra={sorted(actual_inputs - expected_inputs)}"
        )
    if output.exists():
        if not output.is_dir() or any(output.iterdir()):
            fail(f"output must be an empty directory: {output}")
    else:
        output.mkdir(parents=True)
    checksums = {}
    for name in ARCHIVES:
        sidecar = artifacts / f"{name}.sha256"
        regular_file(sidecar)
        match = SIDECAR_LINE.fullmatch(sidecar.read_text(encoding="ascii").strip())
        if not match or match.group(2) not in (None, name):
            fail(f"invalid checksum sidecar: {sidecar.name}")
        checksums[name] = match.group(1)
        regular_file(artifacts / name)
        shutil.copyfile(artifacts / name, output / name)
    # The licence and the third-party notices go inside every archive, as the
    # source's own bytes; they are not separate assets.
    regular_file(source / "LICENSE.md")
    regular_file(source / "THIRD_PARTY_NOTICES.md")
    for target, name in zip(TARGETS, ARCHIVES):
        for required in ("LICENSE.md", "THIRD_PARTY_NOTICES.md"):
            if archived_bytes(output / name, target, required) != (source / required).read_bytes():
                fail(f"{required} inside {name} is not this source's {required}")
    (output / "SHA256SUMS").write_text(
        "".join(f"{checksums[name]}  {name}\n" for name in ARCHIVES), encoding="ascii"
    )
    verify(output)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    stage_parser = sub.add_parser("stage")
    stage_parser.add_argument("--artifacts", type=Path, required=True)
    stage_parser.add_argument("--source", type=Path, required=True)
    stage_parser.add_argument("--out", type=Path, required=True)
    verify_parser = sub.add_parser("verify")
    verify_parser.add_argument("--dir", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "stage":
        stage(args.artifacts, args.source, args.out)
        print(f"staged {len(ASSETS)} public release assets in {args.out}")
    else:
        verify(args.dir)
        print(f"verified {len(ASSETS)} public release assets in {args.dir}")


if __name__ == "__main__":
    main()
