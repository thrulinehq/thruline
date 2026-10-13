//! The product mapping: provider product id → plan and duration.
//!
//! The committed file is `products.toml` at this crate's root. Its ids are
//! placeholders today (no provider account exists), and its plans and
//! durations are T-DIST-4's PROVISIONAL set; its header says both.
//!
//! **An id that is not in the file is refused** ([`Config::grant`] answers
//! `None` and the issuer turns that into [`crate::Refusal::UnknownProduct`]).
//! There is no default plan and there must never be one: a default is how a
//! mis-typed id quietly sells the wrong plan.

use serde::Deserialize;
use thruline_licence::Plan;

/// The committed production mapping, compiled in.
pub const PRODUCTION: &str = include_str!("../products.toml");

/// The longest duration a mapping may give, so a typo of an extra digit is
/// refused at load rather than issued: ten years.
pub const MAX_DAYS: u32 = 3_660;

/// What one product id grants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grant {
    pub plan: Plan,
    pub days: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    providers: Vec<String>,
    accept_test_mode: bool,
    #[serde(default)]
    product: Vec<Row>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    provider: String,
    id: String,
    plan: String,
    days: u32,
}

/// A loaded, checked mapping.
#[derive(Clone, Debug)]
pub struct Config {
    providers: Vec<&'static str>,
    accept_test_mode: bool,
    products: Vec<(&'static str, String, Grant)>,
}

/// Why a mapping file was not loaded. The issuer does not start without one.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("the product mapping is not valid TOML in the expected shape: {0}")]
    Shape(String),
    #[error("the product mapping names provider \"{0}\", which this build does not know")]
    UnknownProvider(String),
    #[error("the product mapping gives \"{id}\" plan \"{plan}\", which is not a ThruLine plan")]
    UnknownPlan { id: String, plan: String },
    #[error("the product mapping gives \"{id}\" {days} days; it must be 1 to {MAX_DAYS}")]
    BadDays { id: String, days: u32 },
    #[error("the product mapping lists {provider} product \"{id}\" twice")]
    Duplicate { provider: String, id: String },
    #[error("the product mapping has a product for {0}, which is not in its providers line")]
    NotEnabled(String),
}

impl Config {
    /// The committed production mapping.
    pub fn production() -> Result<Config, ConfigError> {
        Config::from_toml(PRODUCTION)
    }

    /// A mapping from TOML text, checked in full: every provider known, every
    /// plan a ThruLine plan, every duration in range, no id twice.
    pub fn from_toml(text: &str) -> Result<Config, ConfigError> {
        let file: File = toml::from_str(text).map_err(|e| ConfigError::Shape(e.to_string()))?;
        let known = |name: &str| {
            crate::provider(name)
                .map(|p| p.name())
                .ok_or_else(|| ConfigError::UnknownProvider(name.to_string()))
        };
        let providers = file
            .providers
            .iter()
            .map(|p| known(p))
            .collect::<Result<Vec<_>, _>>()?;
        let mut products: Vec<(&'static str, String, Grant)> = Vec::new();
        for row in file.product {
            let provider = known(&row.provider)?;
            if !providers.contains(&provider) {
                return Err(ConfigError::NotEnabled(row.provider));
            }
            let plan = Plan::parse(&row.plan).ok_or_else(|| ConfigError::UnknownPlan {
                id: row.id.clone(),
                plan: row.plan.clone(),
            })?;
            if row.days == 0 || row.days > MAX_DAYS {
                return Err(ConfigError::BadDays {
                    id: row.id,
                    days: row.days,
                });
            }
            if products
                .iter()
                .any(|(p, id, _)| *p == provider && *id == row.id)
            {
                return Err(ConfigError::Duplicate {
                    provider: row.provider,
                    id: row.id,
                });
            }
            products.push((
                provider,
                row.id,
                Grant {
                    plan,
                    days: row.days,
                },
            ));
        }
        Ok(Config {
            providers,
            accept_test_mode: file.accept_test_mode,
            products,
        })
    }

    /// Whether `provider` is one this issuer accepts webhooks from.
    pub fn enabled(&self, provider: &str) -> bool {
        self.providers.contains(&provider)
    }

    pub fn providers(&self) -> &[&'static str] {
        &self.providers
    }

    pub fn accept_test_mode(&self) -> bool {
        self.accept_test_mode
    }

    /// What `id` grants for `provider`, or `None`. Never a default.
    pub fn grant(&self, provider: &str, id: &str) -> Option<Grant> {
        self.products
            .iter()
            .find(|(p, i, _)| *p == provider && i == id)
            .map(|(_, _, g)| *g)
    }

    /// Every mapped id, for a test that holds the production file to its rules.
    pub fn ids(&self) -> impl Iterator<Item = (&'static str, &str, Grant)> {
        self.products.iter().map(|(p, i, g)| (*p, i.as_str(), *g))
    }
}
