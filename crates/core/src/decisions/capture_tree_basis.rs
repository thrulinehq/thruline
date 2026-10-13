//! Recorded content/index digests reconstructed from immutable Git blobs,
//! never the present checkout. These match capture's recorded manifest rules.
//! The tracked manifest records bytes and file type, but does not observe
//! historical filesystem executable bits. It cannot prove full Git cleanliness.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::chain::{sha256_hex, to_hex};
use sha2::{Digest, Sha256};

pub(crate) struct RecordedTreeContentBasis {
    pub index: String,
    pub tracked: String,
    pub staged: String,
    pub untracked: String,
}

pub(crate) fn from_tree(repo: &Path, tree: &str) -> Option<RecordedTreeContentBasis> {
    let listing = git(
        repo,
        &["ls-tree", "-r", "-z", "--full-tree", tree],
        16 * 1024 * 1024,
    )?;
    let mut entries = BTreeMap::new();
    for record in listing
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let text = std::str::from_utf8(record).ok()?;
        let (meta, path) = text.split_once('\t')?;
        if path.is_empty()
            || path.starts_with('/')
            || path.chars().any(char::is_control)
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return None;
        }
        let fields = meta.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3
            || fields[1] != "blob"
            || !matches!(fields[0], "100644" | "100755" | "120000")
        {
            return None;
        }
        if entries
            .insert(
                path.to_owned(),
                (fields[0].to_owned(), fields[2].to_owned()),
            )
            .is_some()
            || entries.len() > 10_000
        {
            return None;
        }
    }
    let mut index = Vec::new();
    let mut tracked = Sha256::new();
    let mut total = 0usize;
    for (path, (mode, oid)) in entries {
        index.extend_from_slice(format!("{mode} {oid} 0\t{path}\0").as_bytes());
        let size = git(repo, &["cat-file", "-s", &oid], 1024)?;
        let size = std::str::from_utf8(&size)
            .ok()?
            .trim()
            .parse::<usize>()
            .ok()?;
        total = total.checked_add(size)?;
        if size > 2 * 1024 * 1024 || total > 64 * 1024 * 1024 {
            return None;
        }
        let blob = git(repo, &["cat-file", "blob", &oid], size)?;
        if blob.len() != size {
            return None;
        }
        update_len(&mut tracked, path.as_bytes());
        if mode == "120000" {
            let target = std::str::from_utf8(&blob).ok()?;
            if target.starts_with('/') || target.split('/').any(|part| part == "..") {
                return None;
            }
            update_len(&mut tracked, b"symlink");
            update_len(&mut tracked, target.as_bytes());
        } else {
            update_len(&mut tracked, b"file");
            update_len(&mut tracked, sha256_hex(&blob).as_bytes());
        }
    }
    let index = sha256_hex(&index);
    let mut staged = Sha256::new();
    update_len(&mut staged, tree.as_bytes());
    update_len(&mut staged, index.as_bytes());
    Some(RecordedTreeContentBasis {
        index,
        tracked: to_hex(&tracked.finalize()),
        staged: to_hex(&staged.finalize()),
        untracked: sha256_hex(b""),
    })
}

fn update_len(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}

fn git(repo: &Path, args: &[&str], limit: usize) -> Option<Vec<u8>> {
    let mut child = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(args)
        .current_dir(repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > limit {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    child.wait().ok()?.success().then_some(bytes)
}
