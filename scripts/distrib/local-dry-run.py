#!/usr/bin/env python3
"""Stage supplied local build archives and emit a C31 proof; never publish.

Set both CARGO_TARGET_DIR and THRULINE_SHIP_PROOF_TARGET_DIR to the same
absolute external target. This command builds no platform executables and
does not infer platform provenance from archive file names.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import time

spec = importlib.util.spec_from_file_location('ship_proof', Path(__file__).with_name('ship-proof.py'))
proof = importlib.util.module_from_spec(spec)
spec.loader.exec_module(proof)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--archives-dir', type=Path, required=True)
    parser.add_argument('--artifact-kind', choices=('synthetic-fixture', 'release-build-output'), required=True)
    parser.add_argument('--checked-commit')
    args = parser.parse_args()
    owned_dist = None
    try:
        target_value = os.environ.get('CARGO_TARGET_DIR')
        explicit_value = os.environ.get('THRULINE_SHIP_PROOF_TARGET_DIR')
        if not target_value or target_value != explicit_value or not Path(target_value).is_absolute():
            raise proof.Refusal('both explicit target variables must name the same absolute directory')
        target = Path(target_value)
        # Validate the closest existing ancestor before creating output.
        ancestor = target
        while not ancestor.exists() and not ancestor.is_symlink():
            ancestor = ancestor.parent
        proof.ordinary_path(ancestor, directory=True)
        if any(part in ('.', '..') for part in target_value.split('/')):
            raise proof.Refusal('target has nonordinary components')
        target.mkdir(parents=True, exist_ok=True, mode=0o700)
        proof.ordinary_path(target, directory=True)
        head = proof.checked_head(args.repo, args.checked_commit)
        sources = proof.exact_archive_set(args.archives_dir)
        dist = target / 'distrib'
        dist.mkdir(mode=0o700)  # Existing output is never overwritten.
        owned_dist = dist
        deadline = time.monotonic() + 30
        for source in sources:
            sha, before = proof.digest(source, deadline)
            destination = dist / source.name
            with source.open('rb') as input_file, destination.open('xb') as output:
                os.chmod(destination, 0o600)
                amount = 0
                while True:
                    if time.monotonic() >= deadline:
                        raise proof.Refusal('archive staging exceeded time bound')
                    chunk = input_file.read(64 * 1024)
                    if not chunk:
                        break
                    amount += len(chunk)
                    if amount > proof.MAX_BYTES:
                        raise proof.Refusal('archive grew past staging byte bound')
                    output.write(chunk)
            copied_sha, _ = proof.digest(destination, deadline)
            after_sha, after = proof.digest(source, deadline)
            if sha != copied_sha or sha != after_sha or before != after:
                raise proof.Refusal('archive changed while staging')
        data, inventories, families = proof.generate(args.repo, dist, head)
        receipt = {'artifact_kind': args.artifact_kind, 'native_platform_build_proved': False,
                   'checked_commit': data['checked_commit'], 'archive_inventory': inventories,
                   'family_members': families, 'proof': str(dist / 'decisions-ship-proof.json')}
        proof.write_new_json(dist / 'decisions-local-dry-run.json', receipt)
        print(json.dumps(receipt))
        owned_dist = None
    except (OSError, ValueError, tarfile.TarError, subprocess.SubprocessError) as error:
        if owned_dist is not None:
            shutil.rmtree(owned_dist)
        parser.exit(1, 'local dry run refused: ' + str(error) + '\n')


if __name__ == '__main__':
    os.umask(0o077)
    main()
