"""Actual scratch archive tests; fixture executables are never platform builds."""
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
TARGETS = ('aarch64-apple-darwin', 'x86_64-apple-darwin',
           'aarch64-unknown-linux-musl', 'x86_64-unknown-linux-musl')


def fixture(directory):
    repo, archives = directory / 'synthetic-repo', directory / 'fixture-archives'
    repo.mkdir(); archives.mkdir()
    (repo / 'README.md').write_text('Synthetic packaging fixture; no native release build.\n')
    subprocess.run(['git', 'init', '-q', str(repo)], check=True)
    subprocess.run(['git', '-C', str(repo), 'add', 'README.md'], check=True)
    subprocess.run(['git', '-C', str(repo), '-c', 'user.name=fixture',
        '-c', 'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false',
        'commit', '-qm', 'synthetic packaging fixture'], check=True)
    for target in TARGETS:
        archive(archives, target)
    return repo, archives


def archive(directory, target, extra=None, kind=None):
    root = 'thruline-cli-' + target
    with tarfile.open(directory / (root + '.tar.gz'), 'w:gz') as output:
        for name, data in (('thruline', b'SYNTHETIC FIXTURE, NOT A PLATFORM EXECUTABLE\n'),
                           ('LICENSE.md', b'fixture licence\n'),
                           ('THIRD_PARTY_NOTICES.md', b'fixture notices\n')):
            entry = tarfile.TarInfo(root + '/' + name)
            entry.size = len(data); entry.mode = 0o755 if name == 'thruline' else 0o644
            output.addfile(entry, io.BytesIO(data))
        if extra:
            entry = tarfile.TarInfo(root + '/' + extra)
            if kind:
                entry.type = kind; entry.linkname = root + '/thruline'; output.addfile(entry)
            else:
                entry.size = 1; output.addfile(entry, io.BytesIO(b'x'))


class ShipProofTests(unittest.TestCase):
    def setUp(self):
        parent = Path(os.environ.get('CARGO_TARGET_DIR', tempfile.gettempdir()))
        parent.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix='ship-proof-fixture-', dir=parent)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.repo, self.archives = fixture(self.root)
        self.target = self.root / 'external-target'
        self.home = self.root / 'private-home'; self.home.mkdir(mode=0o700)
        self.environment = dict(os.environ, HOME=str(self.home), THRULINE_HOME=str(self.home / '.thruline'),
            CARGO_TARGET_DIR=str(self.target), THRULINE_SHIP_PROOF_TARGET_DIR=str(self.target),
            PYTHONDONTWRITEBYTECODE='1')

    def run_dry(self, *extra):
        return subprocess.run([sys.executable, str(HERE / 'local-dry-run.py'),
            '--repo', str(self.repo), '--archives-dir', str(self.archives),
            '--artifact-kind', 'synthetic-fixture', *extra], env=self.environment,
            capture_output=True, text=True, timeout=30)

    def test_intended_local_dry_run_emits_proof_from_actual_archives(self):
        result = self.run_dry()
        self.assertEqual(result.returncode, 0, result.stderr)
        dist = self.target / 'distrib'
        proof = json.loads((dist / 'decisions-ship-proof.json').read_text())
        self.assertEqual(set(proof), {'schema_version', 'release_version', 'checked_commit', 'source', 'archives'})
        head = subprocess.check_output(['git', '-C', str(self.repo), 'rev-parse', 'HEAD'], text=True).strip()
        self.assertEqual(proof['checked_commit'], head)
        self.assertEqual(proof['release_version'], 'v0.2.0')
        self.assertEqual(proof['source'], 'private-release-dry-run')
        self.assertEqual(len(proof['archives']), 4)
        for item in proof['archives']:
            self.assertEqual(item['sha256'], hashlib.sha256((dist / item['file_name']).read_bytes()).hexdigest())
        receipt = json.loads((dist / 'decisions-local-dry-run.json').read_text())
        self.assertEqual(receipt['artifact_kind'], 'synthetic-fixture')
        self.assertFalse(receipt['native_platform_build_proved'])
        self.assertEqual(set(receipt['family_members']), {
            'docs/**', 'CLAUDE.md', 'LIVE-HANDOFF.md', 'REPORT.md', 'ui/src/hq/screens/*/DATA.md'})
        self.assertTrue(all(not paths for paths in receipt['family_members'].values()))
        self.assertTrue(all(item['member_count'] == 3 for item in receipt['archive_inventory']))
        self.assertFalse((self.repo / 'target').exists())

    def test_shipped_archive_families_never_get_a_proof(self):
        for name in ('extras/docs/plan.md', 'CLAUDE.md', 'LIVE-HANDOFF.md',
                     'REPORT.md', 'ui/src/hq/screens/Inbox/DATA.md'):
            with self.subTest(name=name):
                archive(self.archives, TARGETS[0], extra=name)
                self.target = self.root / ('bad-' + hashlib.sha256(name.encode()).hexdigest()[:8])
                self.environment.update(CARGO_TARGET_DIR=str(self.target),
                                        THRULINE_SHIP_PROOF_TARGET_DIR=str(self.target))
                result = self.run_dry()
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((self.target / 'distrib/decisions-ship-proof.json').exists())

    def test_missing_or_extra_platform_archive_refuses(self):
        first = self.archives / ('thruline-cli-' + TARGETS[0] + '.tar.gz')
        first.unlink()
        self.assertNotEqual(self.run_dry().returncode, 0)
        archive(self.archives, TARGETS[0])
        (self.archives / 'extra.tar.gz').write_bytes(b'extra')
        self.assertNotEqual(self.run_dry().returncode, 0)

    def test_archive_links_and_unsafe_members_refuse(self):
        for name, kind in (('README.md', tarfile.SYMTYPE), ('README.md', tarfile.LNKTYPE),
                           ('../escape', None), ('thruline', None)):
            with self.subTest(name=name, kind=kind):
                archive(self.archives, TARGETS[0], extra=name, kind=kind)
                self.target = self.root / ('unsafe-' + str(len(list(self.root.iterdir()))))
                self.environment.update(CARGO_TARGET_DIR=str(self.target),
                                        THRULINE_SHIP_PROOF_TARGET_DIR=str(self.target))
                self.assertNotEqual(self.run_dry().returncode, 0)

    def test_archive_symlink_or_hardlink_alias_refuses(self):
        first = self.archives / ('thruline-cli-' + TARGETS[0] + '.tar.gz')
        original = first.read_bytes(); first.unlink()
        other = self.root / 'alias-source.tar.gz'; other.write_bytes(original)
        first.symlink_to(other)
        self.assertNotEqual(self.run_dry().returncode, 0)
        first.unlink(); os.link(other, first)
        self.assertNotEqual(self.run_dry().returncode, 0)

    def test_commit_or_explicit_target_mismatch_refuses(self):
        self.assertNotEqual(self.run_dry('--checked-commit', 'f' * 40).returncode, 0)
        self.environment['THRULINE_SHIP_PROOF_TARGET_DIR'] = str(self.root / 'different-target')
        self.assertNotEqual(self.run_dry().returncode, 0)

    def test_existing_distribution_is_not_overwritten(self):
        self.assertEqual(self.run_dry().returncode, 0)
        proof = self.target / 'distrib/decisions-ship-proof.json'
        before = proof.read_bytes()
        self.assertNotEqual(self.run_dry().returncode, 0)
        self.assertEqual(proof.read_bytes(), before)


if __name__ == '__main__':
    if sys.argv[1:2] == ['--fixture']:
        repo, archives = fixture(Path(sys.argv[2]))
        print(json.dumps({'repo': str(repo), 'archives': str(archives), 'artifact_kind': 'synthetic-fixture'}))
    else:
        unittest.main()
