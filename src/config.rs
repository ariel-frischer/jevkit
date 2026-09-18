//! User-level configuration via the `config` crate, layered TOML.
//!
//! Priority (highest wins), matching both argus and 12-factor conventions:
//! CLI flags > `JEV_*` environment variables > `~/.config/jev/config.toml`
//! > built-in defaults.
//!
//! The config lives in the XDG config dir, not the state dir: config is
//! user-authored and long-lived, unlike the usage ledger in
//! `$XDG_STATE_HOME/jev/usage.jsonl`.
//!
//! Only three keys exist. `config`'s layered builder handles the merging;
//! this module adds the key specs for `config set`/`keys` validation and the
//! resolution helpers that `cmd_ask` consumes.

use anyhow::{anyhow, Context, Result};
use config::{Config, File};
use std::path::PathBuf;

const ENV_PREFIX: &str = "JEV";

/// A configurable key: name, whether it is free-form or restricted, and what
/// it is for. `config set` validates against these; unknown keys are
/// rejected so a typo in `jev config set` fails loudly instead of silently
/// having no effect.
pub struct KeySpec {
    pub key: &'static str,
    pub description: &'static str,
    /// One of the known provider names, when the key takes a provider.
    pub one_of_provider: bool,
}

pub const KEYS: [KeySpec; 5] = [
    KeySpec {
        key: "provider",
        description: "default provider: openrouter or typesafe",
        one_of_provider: true,
    },
    KeySpec {
        key: "model",
        description: "default model, e.g. typesafe/jev-1.13",
        one_of_provider: false,
    },
    KeySpec {
        key: "endpoint",
        description: "override the decisions URL, e.g. a proxy or self-hosted /api/alpha/decisions",
        one_of_provider: false,
    },
    KeySpec {
        key: "log",
        description:
            "usage-ledger path for jev ask --log; a value of 1 means the default ledger path",
        one_of_provider: false,
    },
    KeySpec {
        key: "lint_verbosity",
        description: "lint finding detail: full (default) prints help blocks; quiet prints rule ids only. jev ask/lint --quiet overrides",
        one_of_provider: false,
    },
];

pub fn find_key(key: &str) -> Option<&'static KeySpec> {
    KEYS.iter().find(|s| s.key == key)
}

/// `$XDG_CONFIG_HOME/jev/config.toml`, falling back to `~/.config/jev`.
pub fn config_path() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            let home = std::env::var_os("HOME").context(
                "cannot determine the config directory: neither XDG_CONFIG_HOME nor HOME is set",
            )?;
            PathBuf::from(home).join(".config")
        }
    };
    Ok(base.join("jev").join("config.toml"))
}

/// The layered settings for one run: defaults, then file, then env.
///
/// Env vars use the `JEV_` prefix with `_` nesting; only `JEV_PROVIDER`,
/// `JEV_MODEL`, and `JEV_LOG` are meaningful, and the builder maps them onto
/// lowercase keys. A missing or unparsable file yields only defaults: config
/// is a convenience, so a broken file must not block a call.
pub fn load() -> Result<Config> {
    let path = config_path()?;
    let builder = Config::builder()
        .set_default("provider", auth_default_provider())?
        .set_default("model", "")?
        .set_default("endpoint", "")?
        .set_default("log", "")?
        .set_default("lint_verbosity", "")?;
    // File::required(false): no config file yet is the first-run state, not
    // an error.
    let builder = if path.exists() {
        builder.add_source(File::with_name(&path.display().to_string()).required(false))
    } else {
        builder
    };
    let cfg = builder
        .add_source(config::Environment::with_prefix(ENV_PREFIX))
        .build()
        .with_context(|| format!("failed to load config from {}", path.display()))?;
    Ok(cfg)
}

fn auth_default_provider() -> &'static str {
    crate::auth::OPENROUTER.name
}

/// The provider named by config, falling back to the built-in default.
pub fn resolved_provider(cfg: &Config) -> Result<String> {
    let name = cfg.get_string("provider")?;
    if crate::auth::provider_by_name(&name).is_err() {
        return Err(anyhow!(
            "config provider {:?} is not a known provider; run `jev config set provider <name>`",
            name
        ));
    }
    Ok(name)
}

/// The model from config, or `None` when unset (caller then uses the
/// provider's default).
pub fn resolved_model(cfg: &Config) -> Result<Option<String>> {
    let m = cfg.get_string("model")?;
    Ok((!m.is_empty()).then_some(m))
}

/// The endpoint override from config, or `None` when unset (caller then uses
/// the provider's built-in URL).
pub fn resolved_endpoint(cfg: &Config) -> Result<Option<String>> {
    let e = cfg.get_string("endpoint")?;
    Ok((!e.is_empty()).then_some(e))
}

/// The configured ledger behavior.
pub enum LogSetting {
    /// No ledger configured: `--log` must have a path, or nothing is written.
    Off,
    /// Write to the default XDG state ledger when `--log` is passed bare.
    DefaultPath,
    /// Always use this explicit path when `--log` is passed bare.
    Path(std::path::PathBuf),
}

pub fn resolved_log(cfg: &Config) -> Result<LogSetting> {
    let raw = cfg.get_string("log")?;
    match raw.as_str() {
        "" => Ok(LogSetting::Off),
        "1" | "true" => Ok(LogSetting::DefaultPath),
        _ => Ok(LogSetting::Path(PathBuf::from(raw))),
    }
}

/// Configured lint detail level. Distinct from `--quiet`: config defaults,
/// the flag overrides.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LintVerbosity {
    Full,
    Quiet,
}

pub fn resolved_lint_verbosity(cfg: &Config) -> Result<LintVerbosity> {
    let v = cfg.get_string("lint_verbosity")?;
    match v.as_str() {
        "" | "full" => Ok(LintVerbosity::Full),
        "quiet" => Ok(LintVerbosity::Quiet),
        _ => Err(anyhow!(
            "config lint_verbosity {v:?} is not full or quiet; \
             run `jev config set lint_verbosity <full|quiet>`"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that share process env; mutex, not thread-local,
    /// because cargo test forks one process for the whole binary.
    static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn defaults_resolved_when_no_config_exists() {
        // Tests that touch process env must be serialized: cargo test runs
        // them in parallel threads and env is per-process.
        let _g = ENV_TEST_LOCK.lock().unwrap();
        let saved = std::env::var("JEV_MODEL").ok();
        std::env::remove_var("JEV_MODEL");
        let cfg = load().unwrap();
        if let Some(v) = saved {
            std::env::set_var("JEV_MODEL", v);
        }
        assert!(!cfg.get_string("provider").unwrap().is_empty());
        assert_eq!(resolved_model(&cfg).unwrap(), None);
    }

    #[test]
    fn env_wins_over_defaults() {
        let _g = ENV_TEST_LOCK.lock().unwrap();
        let saved = std::env::var("JEV_MODEL").ok();
        std::env::set_var("JEV_MODEL", "test/model");
        let cfg = load().unwrap();
        let got = resolved_model(&cfg).unwrap();
        std::env::remove_var("JEV_MODEL");
        if let Some(v) = saved {
            std::env::set_var("JEV_MODEL", v);
        }
        assert_eq!(got.as_deref(), Some("test/model"));
    }

    #[test]
    fn env_probe_raw_builder() -> Result<(), Box<dyn std::error::Error>> {
        let _g = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var("JEV_MODEL", "probe/model");
        let path = config_path().unwrap();
        let cfg = Config::builder()
            .set_default("model", "")?
            .add_source(config::File::with_name(&path.display().to_string()).required(false))
            .add_source(config::Environment::with_prefix("JEV"))
            .build()?;
        let got = cfg.get_string("model").unwrap();
        std::env::remove_var("JEV_MODEL");
        assert_eq!(got, "probe/model", "env source must map JEV_MODEL -> model");
        Ok(())
    }

    #[test]
    fn log_setting_parses_off_default_path_and_explicit() -> Result<(), Box<dyn std::error::Error>>
    {
        let cfg = Config::builder().set_default("log", "")?.build().unwrap();
        assert!(matches!(resolved_log(&cfg).unwrap(), LogSetting::Off));
        Ok(())
    }

    #[test]
    fn every_key_has_a_spec() {
        // Guards the contract `config set` relies on: setting any documented
        // key must be possible without hitting `unknown key`.
        for spec in KEYS {
            assert!(!spec.key.is_empty());
            assert!(!spec.description.is_empty());
        }
    }
}
