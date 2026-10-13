//! What a verifier requires, stated up front rather than discovered.

use crate::identity::SanKind;
use crate::trustroot::TrustRoot;

/// Who is allowed to have signed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdentityPolicy {
    /// Exact match against the certificate's SAN (email or URI).
    /// `None` means **any identity**, which is almost never what a gate wants.
    pub san: Option<String>,
    /// Which SAN *form* the identity must have arrived in.
    ///
    /// # Why naming the string is not enough
    ///
    /// A Fulcio `otherName` carries arbitrary text, so a certificate can name
    /// the otherName identity `thomas@python.org` — which is not an email
    /// address, it is a username that looks like one. Comparing only the string
    /// lets that certificate satisfy a policy written for the mailbox.
    ///
    /// `None` does **not** mean "any form". It means the policy did not say, and
    /// [`crate::verify`] then refuses an `otherName` match while allowing the
    /// two unambiguous forms. Policing an otherName identity is possible and has
    /// to be asked for by name — see [`Policy::expect_identity_of_kind`].
    pub san_kind: Option<SanKind>,
    /// Exact match against the OIDC issuer extension.
    pub issuer: Option<String>,
}

/// The conditions verification is performed under.
///
/// # Defaults are the strict ones
///
/// `require_inclusion_proof` and `require_sct` default to **true**. A verifier
/// whose safe configuration is the one you have to remember to ask for is a
/// verifier that ships unsafe.
#[derive(Debug, Clone)]
pub struct Policy {
    pub trust_root: TrustRoot,
    pub identity: IdentityPolicy,
    pub require_inclusion_proof: bool,
    pub require_sct: bool,
    /// The instant the certificate's validity window is judged against.
    ///
    /// `None` — the default — means **derive it from the log entry's
    /// `integratedTime`**, which is the only defensible answer for a
    /// certificate that lives ten minutes. Setting it to "now" rejects every
    /// honest bundle; ignoring the window accepts a certificate used long
    /// after issuance.
    pub at_time: Option<time::OffsetDateTime>,
}

impl Policy {
    /// Strict defaults against a given trust root.
    pub fn new(trust_root: TrustRoot) -> Self {
        Policy {
            trust_root,
            identity: IdentityPolicy::default(),
            require_inclusion_proof: true,
            require_sct: true,
            at_time: None,
        }
    }

    /// Require an identity and issuer, leaving the SAN form unstated — which
    /// means an `otherName` cannot satisfy it. See [`IdentityPolicy::san_kind`].
    pub fn expect_identity(mut self, san: impl Into<String>, issuer: impl Into<String>) -> Self {
        self.identity.san = Some(san.into());
        self.identity.issuer = Some(issuer.into());
        self
    }

    /// Require an identity of a specific SAN form. This is how an `otherName`
    /// username identity is policed.
    pub fn expect_identity_of_kind(
        mut self,
        san: impl Into<String>,
        kind: SanKind,
        issuer: impl Into<String>,
    ) -> Self {
        self.identity.san = Some(san.into());
        self.identity.san_kind = Some(kind);
        self.identity.issuer = Some(issuer.into());
        self
    }
}

impl IdentityPolicy {
    /// Does this certificate's identity satisfy the policy?
    ///
    /// Lifted out of [`crate::verify`] so it is reachable by a test without
    /// having to assemble a fully valid bundle around a hostile certificate —
    /// the check that refuses an `otherName` impersonating a mailbox needs a
    /// test that can actually reach it.
    pub fn check(&self, san: &str, kind: SanKind) -> crate::error::Result<()> {
        use crate::error::Error;
        let Some(want) = &self.san else {
            return Ok(());
        };
        if san != want {
            return Err(Error::IdentityMismatch {
                found: san.to_string(),
                expected: want.clone(),
            });
        }
        match self.san_kind {
            Some(want_kind) if want_kind != kind => Err(Error::IdentityKind {
                value: san.to_string(),
                found: kind.as_str(),
                expected: format!("requires {want_kind}"),
            }),
            None if kind == SanKind::OtherName => Err(Error::IdentityKind {
                value: san.to_string(),
                found: kind.as_str(),
                expected: "named an identity without naming its form".to_string(),
            }),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Axis, Error};

    fn kind_of(fixture: &[u8]) -> (String, SanKind) {
        let certs = crate::x509::parse_pem_chain(fixture).unwrap();
        let (san, kind, _issuer) = crate::identity::extract(&certs[0]).unwrap();
        (san, kind)
    }

    /// The forgery this check exists for, on a real hostile certificate: a
    /// Fulcio `otherName` whose contents are the literal string
    /// `thomas@python.org` — a username shaped like the CPython release
    /// manager's mailbox.
    #[test]
    fn an_othername_shaped_like_an_email_does_not_satisfy_a_policy_written_for_the_email() {
        let (san, kind) = kind_of(include_bytes!(
            "../fixtures/identity-othername-email-shaped.pem"
        ));
        assert_eq!(kind, SanKind::OtherName, "the fixture must be an otherName");

        let policy = IdentityPolicy {
            san: Some(san.clone()),
            san_kind: None,
            issuer: None,
        };
        let e = policy
            .check(&san, kind)
            .expect_err("an otherName must not satisfy a policy that named no form");
        assert!(matches!(e, Error::IdentityKind { .. }), "{e}");
        assert_eq!(e.axis(), Axis::Identity);
    }

    /// And it *can* be policed — by asking for it by name.
    #[test]
    fn an_othername_identity_is_policeable_when_the_policy_names_the_form() {
        let (san, kind) = kind_of(include_bytes!(
            "../fixtures/identity-othername-email-shaped.pem"
        ));
        let policy = IdentityPolicy {
            san: Some(san.clone()),
            san_kind: Some(SanKind::OtherName),
            issuer: None,
        };
        policy
            .check(&san, kind)
            .expect("naming the form is how an otherName identity is policed");
    }

    /// The ordinary case still works, or the two tests above would pass against
    /// a check that refuses everything.
    #[test]
    fn a_real_email_identity_satisfies_a_policy_naming_it() {
        let (san, kind) = kind_of(include_bytes!("../fixtures/identity-email-leaf.pem"));
        assert_eq!(kind, SanKind::Email);
        let policy = IdentityPolicy {
            san: Some(san.clone()),
            san_kind: None,
            issuer: None,
        };
        policy
            .check(&san, kind)
            .expect("an rfc822Name identity is unambiguous");
    }

    /// Naming the wrong form is refused even when the string matches.
    #[test]
    fn the_right_string_in_the_wrong_form_is_refused() {
        let (san, kind) = kind_of(include_bytes!("../fixtures/identity-email-leaf.pem"));
        let policy = IdentityPolicy {
            san: Some(san.clone()),
            san_kind: Some(SanKind::Uri),
            issuer: None,
        };
        let e = policy.check(&san, kind).unwrap_err();
        assert!(matches!(e, Error::IdentityKind { .. }), "{e}");
    }
}
