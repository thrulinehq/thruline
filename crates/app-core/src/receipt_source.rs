//! Where §9 says a receipt may come from, and what each source is worth.
//!
//! > *"fetches receipts (from the PR's `.thruline/receipts/` directory, a PR
//! > comment attachment, or an attestation attached via the artifact-attestations
//! > API)"*
//!
//! Those three are not equally trustworthy, and collapsing them into "a receipt
//! was found" would be the App's version of the collapse the whole product is
//! built against. A file in the pull request is content the pull request's
//! author controls; an attestation from GitHub's API is not. Both are usable —
//! a receipt carries its own signature, so its bytes do not have to be trusted
//! to be *found* — but which one was used belongs in the check run, because a
//! reviewer reading "verified" should be able to see what was verified and
//! where it came from.

/// Where a receipt was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// The artifact-attestations API. GitHub vouches for the association
    /// between the attestation and the repository.
    Attestation,
    /// A file in the pull request's tree. The author controls these bytes.
    PullRequestFile,
    /// An attachment on a pull request comment. Anyone who can comment can
    /// attach one.
    Comment,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Attestation => "attestation",
            Source::PullRequestFile => "pull request file",
            Source::Comment => "pull request comment",
        }
    }

    /// What a reader needs to know about this source, in the check run.
    ///
    /// Never omitted for the weaker sources: a receipt's signature is what
    /// makes its *contents* trustworthy, and nothing makes its *presence*
    /// meaningful except knowing who could have put it there.
    pub fn caveat(self) -> Option<&'static str> {
        match self {
            Source::Attestation => None,
            Source::PullRequestFile => Some(
                "this receipt is a file in the pull request, so its author chose which \
                 receipt is here; its signature is what makes its contents \
                 trustworthy, not its presence",
            ),
            Source::Comment => Some(
                "this receipt was attached to a comment, and anyone who can comment on \
                 this pull request could have attached it; its signature is the only \
                 reason to believe its contents",
            ),
        }
    }
}

/// The order the App looks, strongest first.
///
/// Ordering matters and is asserted by a test: looking in the pull request
/// first would mean an author could shadow an attestation by committing a file.
pub const SEARCH_ORDER: &[Source] = &[
    Source::Attestation,
    Source::PullRequestFile,
    Source::Comment,
];

/// A path inside the repository that may hold a receipt.
///
/// Refuses anything that is not directly under `.thruline/receipts/`, because
/// this value ends up joined to a checkout directory. `..`, absolute paths,
/// nested directories and a name that is not `*.json` are all refused rather
/// than normalised — normalising an attack into something safe-looking is how
/// the next normaliser gets it wrong.
pub fn is_receipt_path(path: &str) -> bool {
    let Some(name) = path.strip_prefix(".thruline/receipts/") else {
        return false;
    };
    !name.is_empty()
        && name.ends_with(".json")
        && !name.contains('/')
        && !name.contains('\\')
        && name != ".json"
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}
