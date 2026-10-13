// `npx thruline` — BUILD-SPEC §15's shim. Downloads one pinned, checksummed
// binary for this platform.
//
// **Built here, published elsewhere.** Nothing in this repository publishes to
// npm.
//
// The failure mode this file is written against: a postinstall that half-works
// and leaves a broken `thruline` on PATH. Every step that cannot complete
// throws, and the binary is only moved into place after its checksum matches.
'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { pipeline } = require('stream/promises');
const zlib = require('zlib');

const REPO = 'thrulinehq/thruline';
const VERSION = require('./package.json').version;

// Linux is static musl, for the reason spelled out in ../install.sh: a
// dynamically linked release has to be built against the oldest glibc anyone
// will ever run it on, and that is a guess about machines nobody has seen. The
// bench measured the failure — `libc.so.6: version 'GLIBC_2.39' not found` —
// and a static binary cannot have it.
const TARGETS = {
  'linux-x64': 'x86_64-unknown-linux-musl',
  'linux-arm64': 'aarch64-unknown-linux-musl',
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
};

async function fetchBuffer(url) {
  const res = await fetch(url, { redirect: 'follow' });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText} for ${url}`);
  return Buffer.from(await res.arrayBuffer());
}

async function main() {
  const key = `${process.platform}-${process.arch}`;
  const target = TARGETS[key];
  if (!target) {
    throw new Error(
      `thruline: no published binary for ${key}.`
    );
  }
  // The artifact names are cargo-dist's; `crates/cli/tests/m6_release_paths.rs`
  // pins them against the release plan. The public publisher creates
  // SHA256SUMS from the four final archive hashes.
  const name = `thruline-cli-${target}.tar.gz`;
  const base = `https://github.com/${REPO}/releases/download/v${VERSION}`;
  const [archive, sums] = await Promise.all([
    fetchBuffer(`${base}/${name}`),
    fetchBuffer(`${base}/SHA256SUMS`),
  ]);

  // cargo-dist writes `<hash> *<name>`: the `*` is sha256sum's binary-mode
  // marker and belongs to the field, so it is stripped. Compared for equality
  // and not by `includes`, or the x86_64 archive would satisfy a request for
  // the aarch64 one and the wrong-architecture binary would pass its checksum.
  const want = sums
    .toString('utf8')
    .split('\n')
    .map((l) => l.trim().split(/\s+/))
    .find((p) => p[1] && p[1].replace(/^\*/, '') === name);
  if (!want) throw new Error(`thruline: SHA256SUMS does not list ${name}`);

  const have = crypto.createHash('sha256').update(archive).digest('hex');
  if (have !== want[0]) {
    throw new Error(`thruline: checksum mismatch (want ${want[0]}, got ${have})`);
  }

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'thruline-'));
  // `finally`, not a trailing call. The `rmSync` used to sit at the end of this
  // function, after a `throw` for a malformed archive and an `execFileSync`
  // that fails whenever `tar` does — so it ran on successful installs and on no
  // failing one, and left a directory on the user's machine exactly when the
  // install they were watching had gone wrong. Found by applying Track E's
  // call-site census to this lane.
  try {
    const tgz = path.join(tmp, name);
    fs.writeFileSync(tgz, archive);
    const { execFileSync } = require('child_process');
    execFileSync('tar', ['-xzf', tgz, '-C', tmp]);

    // The archive holds one directory named after itself, with the binary inside.
    const extracted = path.join(tmp, `thruline-cli-${target}`, 'thruline');
    if (!fs.existsSync(extracted)) {
      throw new Error(`thruline: the archive does not contain thruline-cli-${target}/thruline`);
    }

    const binDir = path.join(__dirname, 'bin');
    fs.mkdirSync(binDir, { recursive: true });
    // Moved into place only now: a half-extracted binary must never be reachable
    // under the name people will run.
    fs.copyFileSync(extracted, path.join(binDir, 'thruline-bin'));
    fs.chmodSync(path.join(binDir, 'thruline-bin'), 0o755);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
  void zlib;
  void pipeline;
}

main().catch((e) => {
  console.error(e.message || e);
  process.exit(1);
});
