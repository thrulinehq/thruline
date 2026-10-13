#!/usr/bin/env python3
"""Verify a published binary release and prepare its Homebrew formula PR.

Read-only by default. --create-pr pushes only Formula/thruline.rb to the public
tap and opens a pull request; it never merges one.
"""

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import tempfile
import urllib.error
import urllib.request
from pathlib import Path

from release_assets import ARCHIVES, ASSETS, TARGETS, verify


RELEASE_REPO = "thrulinehq/thruline"
TAP_REPO = "thrulinehq/homebrew-thruline"
API = "https://api.github.com"
ROOT = Path(__file__).resolve().parents[2]
FORMULA_TEMPLATE = ROOT / "packaging/homebrew/Formula/thruline.rb"
TAG = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")


def fail(message: str) -> None:
    raise SystemExit(f"Homebrew tap PR: {message}")


def public_get(url: str):
    request = urllib.request.Request(
        url,
        headers={"Accept": "application/vnd.github+json", "User-Agent": "thruline-release-check"},
    )
    try:
        return urllib.request.urlopen(request, timeout=30)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            fail(f"not publicly accessible: {url}")
        fail(f"GitHub returned HTTP {error.code} for {url}")
    except urllib.error.URLError as error:
        fail(f"cannot reach GitHub: {error.reason}")


def public_json(url: str) -> dict:
    with public_get(url) as response:
        return json.load(response)


def file_sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def fetch_published_release(tag: str, directory: Path) -> dict[str, str]:
    release = public_json(f"{API}/repos/{RELEASE_REPO}/releases/tags/{tag}")
    if (release.get("tag_name") != tag or release.get("draft") is not False
            or release.get("prerelease") is not False or not release.get("published_at")):
        fail(f"{tag} is not a published, stable release")

    assets = release.get("assets", [])
    by_name = {asset.get("name"): asset for asset in assets}
    if len(assets) != len(ASSETS) or len(by_name) != len(ASSETS) or set(by_name) != ASSETS:
        fail("public release assets must be exactly the four archives and SHA256SUMS")

    base = f"https://github.com/{RELEASE_REPO}/releases/download/{tag}/"
    for name in sorted(ASSETS):
        asset = by_name[name]
        url = asset.get("browser_download_url")
        if asset.get("state") != "uploaded" or url != base + name:
            fail(f"unexpected asset URL or state: {name}")
        destination = directory / name
        with public_get(url) as response, destination.open("wb") as output:
            shutil.copyfileobj(response, output)
        if destination.stat().st_size != asset.get("size"):
            fail(f"download size differs from release metadata: {name}")
        digest = asset.get("digest")
        if digest is not None and digest != f"sha256:{file_sha256(destination)}":
            fail(f"GitHub asset digest mismatch: {name}")

    return verify(directory)


def render_formula(tag: str, checksums: dict[str, str]) -> str:
    match = TAG.fullmatch(tag)
    if not match:
        fail("tag must be a plain vMAJOR.MINOR.PATCH version")
    version = tag[1:]
    template = FORMULA_TEMPLATE.read_text(encoding="utf-8")
    anchor = "class Thruline < Formula\n"
    if template.count(anchor) != 1:
        fail("formula template class header is missing or duplicated")
    formula = anchor + template.split(anchor, 1)[1]
    substitutions = {'version "0.0.0-PLACEHOLDER"': f'version "{version}"'}
    for target, archive in zip(TARGETS, ARCHIVES):
        substitutions[f"PLACEHOLDER_{target.upper().replace('-', '_')}"] = checksums[archive]
    for old, new in substitutions.items():
        if formula.count(old) != 1:
            fail(f"formula template placeholder missing or duplicated: {old}")
        formula = formula.replace(old, new)
    if "PLACEHOLDER" in formula:
        fail("formula still contains a placeholder")
    for archive in ARCHIVES:
        if formula.count(f"{RELEASE_REPO}/releases/download/v#{{version}}/{archive}") != 1:
            fail(f"formula URL missing or duplicated: {archive}")
    if re.findall(r'sha256 "([0-9a-f]{64})"', formula) != [checksums[name] for name in ARCHIVES]:
        fail("formula checksums differ from measured public archives")
    return formula


def command(*args: str, cwd: Path) -> None:
    try:
        subprocess.run(args, cwd=cwd, check=True)
    except subprocess.CalledProcessError as error:
        fail(f"{args[0]} failed with exit code {error.returncode}; see output above")


def create_pr(tag: str, formula: str, checksums: dict[str, str], work: Path) -> None:
    for executable in ("git", "gh"):
        if shutil.which(executable) is None:
            fail(f"{executable} is required to open the tap PR")
    tap = public_json(f"{API}/repos/{TAP_REPO}")
    if tap.get("full_name") != TAP_REPO or tap.get("private") is not False:
        fail("the Homebrew tap must exist as a public repository")
    base_branch = tap.get("default_branch")
    if not isinstance(base_branch, str) or not base_branch:
        fail("the Homebrew tap needs an initialized default branch")

    checkout = work / "tap"
    command("git", "clone", f"https://github.com/{TAP_REPO}.git", str(checkout), cwd=work)
    if subprocess.run(("git", "rev-parse", "--verify", "HEAD"), cwd=checkout,
                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
        fail("initialize the tap's default branch before opening a PR")
    branch = f"codex/thruline-{tag}"
    exists = subprocess.run(("git", "ls-remote", "--exit-code", "--heads", "origin", branch),
                            cwd=checkout, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if exists.returncode == 0:
        fail(f"tap branch already exists: {branch}")
    if exists.returncode != 2:
        fail("cannot check whether the tap branch already exists")

    command("git", "switch", "-c", branch, cwd=checkout)
    formula_path = checkout / "Formula/thruline.rb"
    formula_path.parent.mkdir(parents=True, exist_ok=True)
    formula_path.write_text(formula, encoding="utf-8")
    command("git", "add", "--", "Formula/thruline.rb", cwd=checkout)
    changed = subprocess.run(("git", "diff", "--cached", "--quiet"), cwd=checkout)
    if changed.returncode == 0:
        fail("tap formula already matches the published release")
    if changed.returncode != 1:
        fail("cannot inspect the staged tap change")
    command("git", "commit", "-m", f"Update ThruLine formula for {tag}", cwd=checkout)
    command("git", "push", "--set-upstream", "origin", branch, cwd=checkout)

    body = work / "tap-pr-body.md"
    body.write_text(
        f"Update the tap for the published [ThruLine {tag} release]"
        f"(https://github.com/{RELEASE_REPO}/releases/tag/{tag}).\n\n"
        "The four archives were downloaded anonymously. Their SHA-256 values match "
        "the public SHA256SUMS and GitHub asset digests where available.\n\n"
        "| Archive | SHA-256 |\n| --- | --- |\n"
        + "".join(f"| `{name}` | `{checksums[name]}` |\n" for name in ARCHIVES)
        + "\nPlease review and merge manually.\n",
        encoding="utf-8",
    )
    command("gh", "pr", "create", "--repo", TAP_REPO, "--base", base_branch,
            "--head", branch, "--title", f"Update ThruLine formula for {tag}",
            "--body-file", str(body), cwd=checkout)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True, help="published release tag, e.g. v0.1.0")
    parser.add_argument("--output", type=Path, help="also save the rendered formula here")
    parser.add_argument("--create-pr", action="store_true", help="push a formula branch and open a PR")
    args = parser.parse_args()
    if not TAG.fullmatch(args.tag):
        fail("tag must be a plain vMAJOR.MINOR.PATCH version")
    with tempfile.TemporaryDirectory(prefix="thruline-tap-") as temp:
        work = Path(temp)
        release_dir = work / "release"
        release_dir.mkdir()
        checksums = fetch_published_release(args.tag, release_dir)
        formula = render_formula(args.tag, checksums)
        if args.output is not None:
            args.output.write_text(formula, encoding="utf-8")
            print(f"wrote verified formula to {args.output}")
        if args.create_pr:
            create_pr(args.tag, formula, checksums, work)
        else:
            print(f"verified published {args.tag}; pass --create-pr to open a tap PR")


if __name__ == "__main__":
    main()
