//! jevkit: a CLI for TypeSafe's Jev decisions model.
//!
//! Three commands, no more:
//!
//! - `jev ask`   send a question set and print the answers
//! - `jev lint`  validate a question set offline, without spending a call
//! - `jev auth`  manage credentials in the OS keyring
//!
//! On performance: the network dominates completely. Measured against the live
//! endpoint, a call takes ~273 ms (TTFB), while parsing the request takes
//! ~0.16 ms for YAML and ~0.0015 ms for JSON. Local work is under 0.1% of a
//! call, so `ask` is not where speed lives. `lint` is: it touches no network,
//! so linting a large question set is bounded only by local work.

mod auth;
mod client;
mod config;
mod input;
mod lint;
mod types;
mod usage;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use lint::Severity;
use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use types::{Answer, Request};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(
    name = "jev",
    version = VERSION,
    about = "Typed decisions from TypeSafe's Jev model",
    long_about = "Ask Jev typed questions about a piece of text and get back \
                  probabilities, labels, and scores instead of prose.\n\n\
                  A `noul` answer is a probability in [0,1], not a boolean. \
                  Pick a threshold deliberately."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Send a question set to Jev and print the answers
    Ask(AskArgs),
    /// Check a question set offline, without spending an API call
    Lint(LintArgs),
    /// Manage user-level configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Manage stored API credentials
    Auth(AuthArgs),
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Print the config file path
    Path,
    /// Print the effective configuration, one key per line
    Show,
    /// Print one configuration value
    Get {
        /// A configured key; see `jev config keys`
        key: String,
    },
    /// Set one configuration value, creating the config file as needed
    Set {
        /// A configured key; see `jev config keys`
        key: String,
        value: String,
    },
    /// List configurable keys with their meaning
    Keys,
}

#[derive(Args)]
struct AskArgs {
    /// Question set file (YAML or JSON). Use `-` for stdin.
    #[arg(short = 'q', long = "questions", value_name = "FILE")]
    questions: Option<PathBuf>,

    /// The text to judge. Reads stdin when omitted.
    #[arg(value_name = "STATE")]
    state: Option<String>,

    /// Read the state from a file instead of an argument.
    #[arg(
        short = 'f',
        long = "file",
        value_name = "FILE",
        conflicts_with = "state"
    )]
    file: Option<PathBuf>,

    /// Model to use.
    #[arg(short = 'm', long)]
    model: Option<String>,

    /// Provider to call.
    #[arg(short = 'p', long, default_value = "openrouter")]
    provider: String,

    /// API key. Prefer `jev auth login`; a key in argv is visible to other
    /// processes and lands in shell history.
    #[arg(long, value_name = "KEY", env = "JEV_API_KEY", hide_env_values = true)]
    api_key: Option<String>,

    /// Print the full API response rather than a name-to-value summary.
    #[arg(long)]
    raw: bool,

    /// Print the request that would be sent, and exit without sending it.
    #[arg(long)]
    dry_run: bool,

    /// Send even when linting reports problems.
    #[arg(long)]
    no_lint: bool,

    /// Groups related calls for observability. Never sent to the model.
    #[arg(long, value_name = "ID")]
    session_id: Option<String>,

    /// Append this call to the usage ledger. Optional path; defaults to
    /// $XDG_STATE_HOME/jev/usage.jsonl (~/.local/state/jev/usage.jsonl).
    /// Can also be enabled with JEV_LOG_FILE=1 for the default path.
    #[arg(long, value_name = "FILE", num_args = 0..=1, default_missing_value = "")]
    log: Option<String>,
}

#[derive(Args)]
struct LintArgs {
    /// Question set file (YAML or JSON). Reads stdin when omitted.
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    /// Treat warnings as failures. Intended for CI.
    #[arg(long)]
    deny_warnings: bool,

    /// Emit findings as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommand,
}

#[derive(Subcommand)]
enum AuthCommand {
    /// Store an API key in the OS keyring, prompting on a hidden TTY
    Login {
        #[arg(short, long, default_value = "openrouter")]
        provider: String,
    },
    /// Remove a stored API key
    Logout {
        #[arg(short, long, default_value = "openrouter")]
        provider: String,
    },
    /// Show which credential would be used, without revealing it
    Status {
        #[arg(short, long, default_value = "openrouter")]
        provider: String,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Ask(args) => cmd_ask(args),
        Command::Lint(args) => cmd_lint(args),
        Command::Auth(args) => cmd_auth(args),
        Command::Config { command } => cmd_config(command),
    }
}

fn cmd_config(command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Path => {
            println!("{}", config::config_path()?.display());
        }
        ConfigCommand::Show => {
            let cfg = config::load()?;
            for spec in config::KEYS {
                let value = cfg.get_string(spec.key).unwrap_or_default();
                if value.is_empty() {
                    println!("{:<8} (unset)", spec.key);
                } else {
                    println!("{:<8} {}", spec.key, value);
                }
            }
        }
        ConfigCommand::Get { key } => {
            let spec = config::find_key(&key).ok_or_else(|| {
                anyhow::anyhow!("unknown config key {key:?}; see `jev config keys`")
            })?;
            let cfg = config::load()?;
            match cfg.get_string(spec.key) {
                Ok(v) if !v.is_empty() => println!("{v}"),
                _ => println!("{key}: not set"),
            }
        }
        ConfigCommand::Set { key, value } => {
            let spec = config::find_key(&key).ok_or_else(|| {
                anyhow::anyhow!("unknown config key {key:?}; see `jev config keys`")
            })?;
            if spec.one_of_provider && crate::auth::provider_by_name(&value).is_err() {
                bail!("invalid provider {value:?}; expected one of: openrouter, typesafe");
            }
            write_config_key(spec.key, &value)?;
            println!(
                "set {}={} in {}",
                spec.key,
                value,
                config::config_path()?.display()
            );
        }
        ConfigCommand::Keys => {
            for spec in config::KEYS {
                println!("{:<10} {}", spec.key, spec.description);
            }
        }
    }
    Ok(())
}

/// Write one key into the user config file, preserving other settings.
///
/// The file is plain TOML `key = "value"` lines rather than a serde
/// round-trip: the file is user-owned and may hold comments and unknown
/// keys, and a rewrite that drops either would be data loss.
fn write_config_key(key: &str, value: &str) -> Result<()> {
    let path = config::config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let existing = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("failed to read {}", path.display())),
    };
    let line = format!("{key} = \"{value}\"\n");
    let mut out = String::with_capacity(existing.len() + line.len());
    let mut replaced = false;
    for l in existing.lines() {
        let s = l.trim();
        if s.starts_with(key)
            && (s == key || s.starts_with(&format!("{key} ")) || s.starts_with(&format!("{key}=")))
        {
            out.push_str(&line);
            replaced = true;
        } else {
            out.push_str(l);
            out.push('\n');
        }
    }
    if !replaced {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&line);
    }
    std::fs::write(&path, out).with_context(|| format!("failed to write {}", path.display()))
}

fn cmd_ask(args: AskArgs) -> Result<()> {
    // The state and the questions cannot both come from stdin.
    let questions_from_stdin =
        matches!(args.questions.as_deref(), Some(p) if p == std::path::Path::new("-"));
    let state_from_stdin = args.state.is_none() && args.file.is_none();
    if questions_from_stdin && state_from_stdin {
        bail!("both the questions and the state would come from stdin; pass one of them as a file or an argument");
    }

    let question_src = match &args.questions {
        Some(path) => read_source(Some(path))?,
        None => bail!("no questions given; pass --questions FILE"),
    };
    let questions = input::parse_questions(&question_src)?;

    let state_text = match (&args.state, &args.file) {
        (Some(s), _) => s.clone(),
        (None, Some(path)) => read_source(Some(path))?,
        (None, None) => read_source(None)?,
    };

    // Layered settings: CLI flags win, then JEV_* env, then the config file,
    // then the provider's built-in default. Loading is cheap and late: none
    // of this touches the network.
    let defaults = config::load()?;
    let provider_name = if args.provider == "openrouter" {
        // The default_value equals the built-in default, so the flag cannot
        // distinguish "user passed -p openrouter" from "flag untouched".
        // Precedence therefore treats an equal-to-default flag as unset; a
        // genuinely different provider always wins.
        match config::resolved_provider(&defaults) {
            Ok(p) => p,
            Err(e) => {
                // A bad provider in config should not silently win; surface
                // but fall back rather than blocking the call.
                eprintln!("warning: {e:#}");
                args.provider.clone()
            }
        }
    } else {
        args.provider.clone()
    };
    let provider = auth::provider_by_name(&provider_name)?;
    let model = args
        .model
        .or(config::resolved_model(&defaults)?)
        .unwrap_or_else(|| provider.default_model.to_string());

    let session_id_ref = args.session_id.clone();
    let request = Request {
        model,
        state: serde_json::Value::String(state_text),
        questions,
        session_id: args.session_id,
    };

    if !args.no_lint {
        let findings = lint::lint_request(&request);
        let errors: Vec<_> = findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .collect();
        // Warnings go to stderr so they never corrupt piped JSON output.
        for f in findings.iter().filter(|f| f.severity == Severity::Warning) {
            eprintln!("warning[{}]: {}: {}", f.rule, f.path, f.message);
        }
        if !errors.is_empty() {
            for f in &errors {
                eprintln!("error[{}]: {}: {}", f.rule, f.path, f.message);
                if let Some(help) = &f.help {
                    eprintln!("  help: {help}");
                }
            }
            bail!(
                "{} problem(s) would cause a failed or wasted call; fix them or pass --no-lint",
                errors.len()
            );
        }
    }

    if args.dry_run {
        println!("{}", serde_json::to_string_pretty(&request)?);
        return Ok(());
    }

    let (key, _source) = auth::resolve(provider, args.api_key.as_deref())?;
    let client = client::Client::new(provider.endpoint, key)?;
    let started = std::time::Instant::now();
    let result = client.decide(&request);
    let elapsed_ms = started.elapsed().as_millis();

    // Ledger resolution: an explicit --log path wins; a bare --log consults
    // the config's log setting, then the default XDG state path.
    let log_arg = args.log.clone();
    let ledger_requested = log_arg.is_some()
        || std::env::var_os("JEV_LOG_FILE").is_some()
        || matches!(
            config::resolved_log(&defaults)?,
            config::LogSetting::DefaultPath | config::LogSetting::Path(_)
        );
    if ledger_requested {
        let record_request = serde_json::to_value(&request)
            .context("failed to serialize the request for the usage ledger")?;
        let ledger_path = match log_arg
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
        {
            Some(p) => p,
            None => match config::resolved_log(&defaults)? {
                config::LogSetting::Path(p) => p,
                _ => match std::env::var_os("JEV_LOG_FILE").map(std::path::PathBuf::from) {
                    Some(p) => p,
                    None => usage::resolve_log_path(None)?,
                },
            },
        };
        let call_result = {
            let meta = usage::CallMeta {
                provider: provider.name,
                endpoint: provider.endpoint,
                model: &request.model,
                session_id: session_id_ref.as_deref(),
                elapsed_ms,
            };
            match &result {
                Ok(response) => {
                    usage::log_call(&ledger_path, &meta, &record_request, Some(response), "ok")
                }
                Err(err) => usage::log_call(
                    &ledger_path,
                    &meta,
                    &record_request,
                    Option::<&serde_json::Value>::None,
                    "error",
                )
                .map(|_| {
                    eprintln!("note: call error was: {err:#}");
                }),
            }
        };
        if let Err(e) = call_result {
            eprintln!("warning: could not write the usage ledger: {e:#}");
        } else if result.is_ok() {
            eprintln!("usage ledger: {}", ledger_path.display());
        }
    }

    let response = result?;

    if args.raw {
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }

    // Default output: one value per question, keyed by name. This is the shape
    // a script wants, and it is stable across primitives.
    let mut summary = serde_json::Map::new();
    for (name, answer) in &response.answers {
        let value = match answer {
            Answer::Noul { noul } => serde_json::json!(noul),
            Answer::Choice { choice, .. } => serde_json::json!(choice),
            Answer::Score { score, .. } => serde_json::json!(score),
        };
        summary.insert(name.clone(), value);
    }
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn cmd_lint(args: LintArgs) -> Result<()> {
    let src = read_source(args.file.as_deref())?;
    let questions = input::parse_questions(&src)?;
    let findings = lint::lint_questions(&questions);

    if args.json {
        let payload: Vec<_> = findings
            .iter()
            .map(|f| {
                serde_json::json!({
                    "severity": f.severity.to_string(),
                    "rule": f.rule,
                    "path": f.path,
                    "message": f.message,
                    "help": f.help,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else if findings.is_empty() {
        println!("{} question(s), no problems found", questions.len());
    } else {
        for f in &findings {
            println!("{}[{}]: {}: {}", f.severity, f.rule, f.path, f.message);
            if let Some(help) = &f.help {
                println!("  help: {help}");
            }
        }
    }

    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warnings = findings.len() - errors;
    if errors > 0 || (args.deny_warnings && warnings > 0) {
        std::process::exit(1);
    }
    Ok(())
}

fn cmd_auth(args: AuthArgs) -> Result<()> {
    match args.command {
        AuthCommand::Login { provider } => {
            let provider = auth::provider_by_name(&provider)?;
            if !std::io::stdin().is_terminal() {
                bail!(
                    "`jev auth login` needs a terminal to prompt for the key.\n\
                     For non-interactive use, set {}.",
                    provider.env_var
                );
            }
            // Prompted, never taken from argv: arguments are visible in the
            // process list and in shell history.
            print!("API key for {}: ", provider.name);
            std::io::stdout().flush().ok();
            let key = rpassword::read_password().context("failed to read the key")?;
            let key = key.trim();
            if key.is_empty() {
                bail!("no key entered");
            }
            auth::keyring_set(provider, key)?;
            println!("Stored in the OS keyring ({}).", auth::fingerprint(key));
        }
        AuthCommand::Logout { provider } => {
            let provider = auth::provider_by_name(&provider)?;
            if auth::keyring_delete(provider)? {
                println!("Removed the stored key for {}.", provider.name);
            } else {
                println!("No stored key for {}.", provider.name);
            }
        }
        AuthCommand::Status { provider } => {
            let provider = auth::provider_by_name(&provider)?;
            println!("provider: {}", provider.name);
            println!("endpoint: {}", provider.endpoint);
            println!("model:    {}", provider.default_model);
            match auth::resolve(provider, None) {
                // Only ever a fingerprint: enough to tell two keys apart, not
                // enough to use one.
                Ok((key, source)) => {
                    println!("key:      found via {source} ({})", auth::fingerprint(&key));
                }
                Err(_) => {
                    println!("key:      not configured");
                    println!(
                        "          run `jev auth login --provider {}`, or set {}",
                        provider.name, provider.env_var
                    );
                }
            }
        }
    }
    Ok(())
}

/// Read from a path, or from stdin when the path is `None` or `-`.
fn read_source(path: Option<&std::path::Path>) -> Result<String> {
    match path {
        Some(p) if p != std::path::Path::new("-") => {
            std::fs::read_to_string(p).with_context(|| format!("failed to read {}", p.display()))
        }
        _ => {
            if std::io::stdin().is_terminal() {
                bail!("no input given, and stdin is a terminal");
            }
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("failed to read stdin")?;
            Ok(buf)
        }
    }
}
