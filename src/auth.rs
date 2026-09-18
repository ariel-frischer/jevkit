//! Credential storage.
//!
//! Precedence: explicit `--api-key`, then the environment, then the OS
//! keyring.
//!
//! The keyring is the default for interactive use because an environment
//! variable leaks: it is visible to every child process, lands in shell
//! history when exported inline, and gets captured by any tool that dumps the
//! environment for debugging. The env var remains supported for CI, where a
//! keyring usually does not exist.
//!
//! A key is never accepted as a command-line argument value from the user:
//! `jev auth login` prompts on a hidden TTY, because argv is world-readable on
//! most systems and lands in shell history.

use anyhow::{anyhow, Context, Result};

const SERVICE: &str = "jevkit";

/// Where a key came from, for `jev auth status`. Never carries the key itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Flag,
    Env(&'static str),
    Keyring,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Flag => write!(f, "--api-key flag"),
            Source::Env(name) => write!(f, "environment ({name})"),
            Source::Keyring => write!(f, "OS keyring"),
        }
    }
}

/// A provider's endpoint, default model, and credential names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider {
    pub name: &'static str,
    pub endpoint: &'static str,
    pub default_model: &'static str,
    pub env_var: &'static str,
}

pub const OPENROUTER: Provider = Provider {
    name: "openrouter",
    endpoint: "https://openrouter.ai/api/alpha/decisions",
    default_model: crate::types::DEFAULT_MODEL,
    env_var: "OPENROUTER_API_KEY",
};

pub const TYPESAFE: Provider = Provider {
    name: "typesafe",
    endpoint: "https://api.typesafe.ai/v1/systemone",
    default_model: "jev-latest",
    env_var: "TYPESAFE_API_KEY",
};

pub const PROVIDERS: [Provider; 2] = [OPENROUTER, TYPESAFE];

pub fn provider_by_name(name: &str) -> Result<Provider> {
    PROVIDERS
        .iter()
        .find(|p| p.name == name)
        .copied()
        .ok_or_else(|| {
            anyhow!(
                "unknown provider {name:?}; expected one of: {}",
                PROVIDERS
                    .iter()
                    .map(|p| p.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// Resolve a key, reporting where it came from.
pub fn resolve(provider: Provider, flag: Option<&str>) -> Result<(String, Source)> {
    if let Some(key) = flag.filter(|k| !k.is_empty()) {
        return Ok((key.to_string(), Source::Flag));
    }
    if let Ok(key) = std::env::var(provider.env_var) {
        if !key.trim().is_empty() {
            return Ok((key, Source::Env(provider.env_var)));
        }
    }
    match keyring_get(provider) {
        Ok(Some(key)) => Ok((key, Source::Keyring)),
        Ok(None) => Err(anyhow!(
            "no API key for provider {p}.\n\
             Run `jev auth login --provider {p}`, or set {env}.",
            p = provider.name,
            env = provider.env_var
        )),
        Err(e) => Err(e),
    }
}

/// Look up a stored key. `Ok(None)` means no entry, which is not an error.
pub fn keyring_get(provider: Provider) -> Result<Option<String>> {
    let entry =
        keyring::Entry::new(SERVICE, provider.name).context("failed to open the OS keyring")?;
    match entry.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("failed to read from the OS keyring")),
    }
}

pub fn keyring_set(provider: Provider, key: &str) -> Result<()> {
    let entry =
        keyring::Entry::new(SERVICE, provider.name).context("failed to open the OS keyring")?;
    entry
        .set_password(key)
        .context("failed to write to the OS keyring")
}

/// Remove a stored key. Returns false when there was nothing to remove.
pub fn keyring_delete(provider: Provider) -> Result<bool> {
    let entry =
        keyring::Entry::new(SERVICE, provider.name).context("failed to open the OS keyring")?;
    match entry.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(anyhow::Error::new(e).context("failed to delete from the OS keyring")),
    }
}

/// A non-reversible fingerprint for confirming *which* key is in use without
/// disclosing it. Shows only the length and the last four characters, which is
/// enough to tell two keys apart and not enough to use one.
pub fn fingerprint(key: &str) -> String {
    let n = key.chars().count();
    let tail: String = key.chars().skip(n.saturating_sub(4)).collect();
    format!("{n} chars, ending {tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_hides_the_key() {
        let fp = fingerprint("sk-or-v1-abcdefghijklmnop");
        assert!(fp.contains("mnop"));
        assert!(!fp.contains("abcdefghij"));
    }

    #[test]
    fn fingerprint_handles_short_keys() {
        assert_eq!(fingerprint("ab"), "2 chars, ending ab");
        assert_eq!(fingerprint(""), "0 chars, ending ");
    }

    #[test]
    fn flag_beats_env() {
        std::env::set_var(OPENROUTER.env_var, "from-env");
        let (key, source) = resolve(OPENROUTER, Some("from-flag")).unwrap();
        assert_eq!(key, "from-flag");
        assert_eq!(source, Source::Flag);
        std::env::remove_var(OPENROUTER.env_var);
    }

    #[test]
    fn unknown_provider_lists_valid_ones() {
        let err = provider_by_name("nope").unwrap_err().to_string();
        assert!(err.contains("openrouter"));
    }
}
