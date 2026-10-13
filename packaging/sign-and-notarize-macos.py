#!/usr/bin/env python3
"""Sign and notarize one cargo-dist macOS archive before it can be published.

The notarization submission is a temporary ZIP of the signed executable. The
published artifact remains cargo-dist's tar.gz with its original name/layout.
After signing changes the executable, both cargo-dist checksum records are
updated: the .sha256 sidecar and the build manifest used for sha256.sum.

The script imports APPLE_DEVELOPER_ID_P12_BASE64 into a temporary keychain and
uses an App Store Connect team API key. It refuses missing credentials.
"""

from __future__ import annotations

import base64
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys
import tarfile
import tempfile


TEAM_ID = "6RJD5V8NSN"
SIGNING_IDENTIFIER = "com.thrulinehq.thruline"
# Public fingerprint of the current Developer ID Application certificate. A
# renewal changes this deliberately in code instead of silently selecting a
# different identity from an imported P12.
DEVELOPER_ID_SHA1 = "6F2B56EF655DAC1E8A4F3E0F0D79D3A304907601"
DEVELOPER_ID_G2_URL = "https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer"
DEVELOPER_ID_G2_SHA256 = "f16cd3c54c7f83cea4bf1a3e6a0819c8aaa8e4a1528fd144715f350643d2df3a"
TARGETS = {"aarch64-apple-darwin", "x86_64-apple-darwin"}
ARCHIVE_CONTENTS = {"thruline", "README.md", "CHANGELOG.md", "LICENSE.md", "THIRD_PARTY_NOTICES.md"}


class ReleaseError(Exception):
    pass


def redact(value: str, sensitive: tuple[str, ...] = ()) -> str:
    for secret in sensitive:
        if secret:
            value = value.replace(secret, "[REDACTED]")
    for name in (
        "APPLE_DEVELOPER_ID_P12_BASE64",
        "APPLE_DEVELOPER_ID_P12_PASSWORD",
        "APPLE_NOTARY_KEY_P8_BASE64",
        "APPLE_NOTARY_KEY_ID",
        "APPLE_NOTARY_ISSUER_ID",
    ):
        secret = os.environ.get(name, "")
        if secret:
            value = value.replace(secret, "[REDACTED]")
    return value


def run(
    command: list[str], *, env: dict[str, str] | None = None,
    sensitive: tuple[str, ...] = (),
) -> subprocess.CompletedProcess[str]:
    """Run a tool without putting secret-bearing command arguments in errors."""
    result = subprocess.run(command, text=True, capture_output=True, env=env)
    if result.returncode:
        details = redact((result.stderr or result.stdout).strip(), sensitive)
        raise ReleaseError(f"{Path(command[0]).name} failed: {details}")
    return result


def required_env(name: str) -> str:
    value = os.environ.get(name, "").strip()
    if not value:
        raise ReleaseError(f"{name} is required")
    return value


def decode_secret(name: str, path: Path) -> None:
    value = "".join(required_env(name).split())
    try:
        data = base64.b64decode(value, validate=True)
    except ValueError as exc:
        raise ReleaseError(f"{name} is not valid base64") from exc
    if not data:
        raise ReleaseError(f"{name} decoded to an empty file")
    path.write_bytes(data)
    path.chmod(0o600)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def replace_file(path: Path, data: bytes) -> None:
    """Replace a dist output on the same filesystem, preserving its mode."""
    handle, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(handle, "wb") as stream:
            stream.write(data)
        os.chmod(temporary, path.stat().st_mode & 0o777)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def check_archive(path: Path, root: str) -> None:
    with tarfile.open(path, "r:gz") as archive:
        entries = archive.getmembers()
        members = {member.name.rstrip("/"): member for member in entries}
    if len(members) != len(entries):
        raise ReleaseError(f"duplicate path in {path.name}")
    expected = {root, *(f"{root}/{name}" for name in ARCHIVE_CONTENTS)}
    if set(members) != expected:
        raise ReleaseError(f"unexpected archive layout in {path.name}: {sorted(members)}")
    if not members[root].isdir() or any(
        not members[f"{root}/{name}"].isfile() for name in ARCHIVE_CONTENTS
    ):
        raise ReleaseError(f"archive contains an unexpected file type: {path.name}")


def check_codesign(binary: Path) -> None:
    verified = run(["/usr/bin/codesign", "--verify", "--strict", "--verbose=2", str(binary)])
    output = redact((verified.stdout + verified.stderr).strip())
    if output:
        print(output)
    info = run(["/usr/bin/codesign", "--display", "--verbose=4", str(binary)])
    details = info.stdout + info.stderr
    checks = {
        f"TeamIdentifier={TEAM_ID}": "Developer Team ID",
        f"Identifier={SIGNING_IDENTIFIER}": "code signing identifier",
        "Authority=Developer ID Application:": "Developer ID Application certificate",
    }
    for needle, label in checks.items():
        if needle not in details:
            raise ReleaseError(f"signed executable has the wrong {label}")
    if not re.search(r"\bflags=0x[0-9a-fA-F]+\([^)]*\bruntime\b", details):
        raise ReleaseError("signed executable lacks hardened runtime")
    if not re.search(r"^Timestamp=(?!none$).+", details, re.MULTILINE):
        raise ReleaseError("signed executable lacks a secure timestamp")
    print(f"codesign --verify --strict: passed ({binary.name}, {TEAM_ID}, hardened runtime, timestamp)")


def notarize(binary: Path, work: Path, auth: list[str]) -> str:
    submission = work / "notarization.zip"
    run(["/usr/bin/ditto", "-c", "-k", "--keepParent", str(binary), str(submission)])
    response = run(
        [
            "/usr/bin/xcrun", "notarytool", "submit", str(submission),
            *auth, "--wait", "--timeout", "60m", "--output-format", "json",
        ]
    )
    try:
        result = json.loads(response.stdout)
    except json.JSONDecodeError as exc:
        raise ReleaseError("notarytool did not return a JSON result") from exc
    submission_id = result.get("id", "unknown")
    status = result.get("status")
    if status != "Accepted":
        raise ReleaseError(
            f"Apple notarization status is {status!r} (submission {submission_id}); "
            "retrieve its log with xcrun notarytool log"
        )
    print(f"notarytool status=Accepted; submission {submission_id}")
    return str(submission_id)


def main() -> None:
    if sys.platform != "darwin":
        raise ReleaseError("Apple signing must run on macOS")
    if len(sys.argv) != 3 or sys.argv[1] not in TARGETS:
        raise ReleaseError("usage: sign-and-notarize-macos.py <apple-target> <dist-manifest.json>")

    target = sys.argv[1]
    manifest_path = Path(sys.argv[2]).resolve()
    name = f"thruline-cli-{target}.tar.gz"
    archive = Path("target/distrib") / name
    checksum_path = archive.with_name(name + ".sha256")
    required_env("APPLE_DEVELOPER_ID_P12_BASE64")
    required_env("APPLE_DEVELOPER_ID_P12_PASSWORD")
    required_env("APPLE_NOTARY_KEY_P8_BASE64")
    required_env("APPLE_NOTARY_KEY_ID")
    required_env("APPLE_NOTARY_ISSUER_ID")

    if not archive.is_file() or not checksum_path.is_file() or not manifest_path.is_file():
        raise ReleaseError("cargo-dist archive, checksum, or manifest is missing")
    manifest = json.loads(manifest_path.read_text())
    artifact = manifest.get("artifacts", {}).get(name)
    if not artifact or artifact.get("name") != name:
        raise ReleaseError(f"manifest does not describe {name}")
    if Path(artifact.get("path", "")).resolve() != archive.resolve():
        raise ReleaseError(f"manifest archive path is not {archive}")
    if artifact.get("checksum") != checksum_path.name:
        raise ReleaseError(f"manifest does not link {name} to its checksum sidecar")
    uploads = set(manifest.get("upload_files", []))
    if str(archive.resolve()) not in uploads or str(checksum_path.resolve()) not in uploads:
        raise ReleaseError("manifest does not upload the archive and its checksum")
    current_hash = sha256(archive)
    if artifact.get("checksums", {}).get("sha256") != current_hash:
        raise ReleaseError("archive disagrees with cargo-dist manifest checksum")
    sidecar_lines = [line for line in checksum_path.read_text().splitlines() if line.strip()]
    if sidecar_lines != [f"{current_hash} *{name}"]:
        raise ReleaseError("archive disagrees with cargo-dist .sha256 sidecar")
    root = name.removesuffix(".tar.gz")
    check_archive(archive, root)

    with tempfile.TemporaryDirectory(prefix="thruline-apple-sign-") as temp:
        work = Path(temp)
        stage = work / "stage"
        stage.mkdir()
        unpack_env = dict(os.environ, COPYFILE_DISABLE="1")
        run(["/usr/bin/tar", "-xzf", str(archive.resolve()), "-C", str(stage)], env=unpack_env)
        binary = stage / root / "thruline"
        keychain = work / "signing.keychain-db"
        user_keychains = [
            line.strip().strip('"') for line in
            run(["/usr/bin/security", "list-keychains", "-d", "user"]).stdout.splitlines()
            if line.strip()
        ]
        try:
            sign_args = [
                "/usr/bin/codesign", "--force", "--options", "runtime", "--timestamp",
                "--identifier", SIGNING_IDENTIFIER, "--sign", DEVELOPER_ID_SHA1,
            ]
            password = secrets.token_urlsafe(32)
            certificate = work / "certificate.p12"
            decode_secret("APPLE_DEVELOPER_ID_P12_BASE64", certificate)
            run(["/usr/bin/security", "create-keychain", "-p", password, str(keychain)], sensitive=(password,))
            run(["/usr/bin/security", "set-keychain-settings", "-lut", "21600", str(keychain)])
            run(["/usr/bin/security", "unlock-keychain", "-p", password, str(keychain)], sensitive=(password,))
            intermediate = work / "DeveloperIDG2CA.cer"
            run([
                "/usr/bin/curl", "--proto", "=https", "--tlsv1.2", "--fail", "--silent",
                "--show-error", "--location", "--max-time", "30", DEVELOPER_ID_G2_URL,
                "--output", str(intermediate),
            ])
            if sha256(intermediate) != DEVELOPER_ID_G2_SHA256:
                raise ReleaseError("Apple Developer ID G2 intermediate has an unexpected checksum")
            run(["/usr/bin/security", "import", str(intermediate), "-k", str(keychain), "-t", "cert"])
            run([
                "/usr/bin/security", "import", str(certificate), "-k", str(keychain),
                "-P", required_env("APPLE_DEVELOPER_ID_P12_PASSWORD"),
                "-f", "pkcs12", "-T", "/usr/bin/codesign",
            ])
            run([
                "/usr/bin/security", "set-key-partition-list", "-S",
                "apple-tool:,apple:,codesign:", "-k", password, str(keychain),
            ], sensitive=(password,))
            run([
                "/usr/bin/security", "list-keychains", "-d", "user", "-s",
                str(keychain), *user_keychains,
            ])
            identities = run([
                "/usr/bin/security", "find-identity", "-v", "-p", "codesigning", str(keychain),
            ]).stdout
            if DEVELOPER_ID_SHA1 not in identities:
                raise ReleaseError("temporary keychain lacks the pinned valid signing identity")
            sign_args.extend(["--keychain", str(keychain)])
            run([*sign_args, str(binary)])
            check_codesign(binary)
        finally:
            if keychain.exists():
                try:
                    if user_keychains:
                        run(["/usr/bin/security", "list-keychains", "-d", "user", "-s", *user_keychains])
                finally:
                    run(["/usr/bin/security", "delete-keychain", str(keychain)])

        signed_hash = sha256(binary)
        private_key = work / "notary-key.p8"
        decode_secret("APPLE_NOTARY_KEY_P8_BASE64", private_key)
        auth = [
            "--key", str(private_key), "--key-id", required_env("APPLE_NOTARY_KEY_ID"),
            "--issuer", required_env("APPLE_NOTARY_ISSUER_ID"),
        ]
        submission_id = notarize(binary, work, auth)

        repacked = work / name
        run(["/usr/bin/tar", "-czf", str(repacked), "-C", str(stage), root], env=unpack_env)
        check_archive(repacked, root)
        verify_dir = work / "verify"
        verify_dir.mkdir()
        run(["/usr/bin/tar", "-xzf", str(repacked), "-C", str(verify_dir)], env=unpack_env)
        published_binary = verify_dir / root / "thruline"
        if sha256(published_binary) != signed_hash:
            raise ReleaseError("repacked executable differs from the notarized executable")
        check_codesign(published_binary)
        new_hash = sha256(repacked)

        # Only mutate dist outputs once Apple has accepted the signed binary.
        replace_file(archive, repacked.read_bytes())
        replace_file(checksum_path, f"{new_hash} *{name}\n".encode())
        artifact["checksums"]["sha256"] = new_hash
        replace_file(manifest_path, (json.dumps(manifest, indent=2) + "\n").encode())
        if sha256(archive) != new_hash:
            raise ReleaseError("published archive checksum changed unexpectedly")
        print(f"Archive layout: {root}/{{thruline,README.md,CHANGELOG.md,LICENSE.md,THIRD_PARTY_NOTICES.md}}")
        print(f"Signed and notarized {name}; submission {submission_id}; SHA-256 {new_hash}")


if __name__ == "__main__":
    try:
        main()
    except ReleaseError as exc:
        print(f"Apple release signing failed: {exc}", file=sys.stderr)
        sys.exit(1)
