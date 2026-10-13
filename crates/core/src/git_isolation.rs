//! One place that decides what git configuration a spawned `git` may read.
//!
//! # Why this exists
//!
//! The engine's tree identity is computed by asking `git`. If `git` reads a
//! configuration the engine did not choose, the answer changes and nothing
//! says so. Track R demonstrated exactly that (F17(b)): a real `~/.gitconfig`
//! carrying `core.autocrlf=true` moved a receipt's `subject.gitTree` from
//! `13f4b50…` to `f1cb474…` — LF bytes against CRLF on disk — while the
//! receipt still reported `capture_degraded:false` and `content_filters:null`.
//! No planted file, no privilege: an ordinary developer setting silently
//! rebound the subject of every receipt.
//!
//! A second route was closed at the same time: `capture::git_output` pointed
//! `GIT_CONFIG_GLOBAL` at a fixed, world-writable path in `/tmp`, so anyone
//! able to create that path chose the config instead.
//!
//! # What it does, and what it deliberately does not
//!
//! `GIT_CONFIG_NOSYSTEM=1` drops `/etc/gitconfig`; `GIT_CONFIG_GLOBAL=/dev/null`
//! drops `~/.gitconfig` without naming a file anyone can write. **Repository**
//! config (`.git/config`) and `.gitattributes` are untouched — they are part of
//! the tree under review, and a receipt that ignored them would describe a
//! repository nobody has.
//!
//! This is not a general "make git hermetic" helper. It is the answer to one
//! question — *may this subprocess be steered by config the engine did not
//! choose?* — and two callers answer no differently, by name, in
//! `git_spawn_census.rs`.

use std::process::Command;

/// Refuse system and user git configuration for one spawned `git`.
///
/// Applied to every product `git` subprocess except the two the census names.
pub fn isolate_config(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
}
