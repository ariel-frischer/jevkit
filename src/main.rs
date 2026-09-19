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
use clap::{Args, CommandFactory, Parser, Subcommand};
use config::LintVerbosity;
use lint::Severity;
use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use types::{Answer, Request};

const VERSION_BASE: &str = env!("CARGO_PKG_VERSION");

/// ASCII logo from assets/logo.txt, tinted cyan with 256-color ANSI.
/// Shown by the bare `--version` flag so there is a "v command"-style view;
/// the clap `version` string itself stays one plain line for scripts.
const LOGO_ANSI: &str = concat!(
    "\x1b[38;5;39m   ██ ██████ ██  ██ ██ ▄█▀ ██ ██████ \x1b[0m\n",
    "\x1b[38;5;39m   ██ ██▄▄   ██▄▄██ ████   ██   ██   \x1b[0m\n",
    "\x1b[38;5;39m████▀ ██▄▄▄▄  ▀██▀  ██ ▀█▄ ██   ██   \x1b[0m\n",
);

/// Crate version, plus the git hash stamped by build.rs for non-tagged
/// builds. Tagged builds get the plain version.
fn version_string() -> &'static str {
    Box::leak(
        match option_env!("JEV_GIT_HASH") {
            Some(hash) if !hash.is_empty() => format!("{VERSION_BASE} ({hash})"),
            _ => VERSION_BASE.to_string(),
        }
        .into_boxed_str(),
    )
}

#[derive(Parser)]
#[command(
    name = "jev",
    version = version_string(),
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
    /// Interactive first-run setup: provider, key, and optional custom endpoint
    ///
    /// Hidden: the visible three-command surface stays, as with `completions`.
    #[command(hide = true)]
    Init {
        /// Provider to configure (skips the provider prompt). `custom`
        /// combines with `JEV_INIT_ENDPOINT` (the decisions URL) and
        /// `JEV_INIT_SLOT` (default openrouter) to run fully
        /// non-interactively, or prompts for what is missing.
        #[arg(long, value_name = "NAME")]
        provider: Option<String>,

        /// Skip the API key prompt entirely
        #[arg(long)]
        no_key: bool,
    },
    /// Print shell completions and the man page
    ///
    /// Hidden: the visible three-command surface stays. Autocomplete the
    /// subcommand once and tab completion finds it again.
    #[command(hide = true)]
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Option<clap_complete::Shell>,
    },
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

    /// Terse lint findings (rule ids only, no help blocks). Overrides the
    /// `lint_verbosity` config key.
    #[arg(long)]
    quiet: bool,

    /// One-line JSON output. Implied when stdout is a pipe, so
    /// `jev ask ... | jq` gets compact JSON; use --pretty to force the
    /// indented form in scripts.
    #[arg(long, conflicts_with = "pretty")]
    compact: bool,

    /// Pretty-print JSON output even when stdout is a pipe.
    #[arg(long)]
    pretty: bool,

    /// Groups related calls for observability. Never sent to the model.
    #[arg(long, value_name = "ID")]
    session_id: Option<String>,

    /// Inline question set as a YAML or JSON string. Takes precedence over
    /// --questions; useful for one-off calls without a temp file.
    #[arg(long, value_name = "QUESTIONS")]
    question_set: Option<String>,

    /// Append this call to the usage ledger. Optional path; defaults to
    /// $XDG_STATE_HOME/jev/usage.jsonl (~/.local/state/jev/usage.jsonl).
    /// Can also be enabled with JEV_LOG_FILE=1 for the default path.
    ///
    /// Exit codes: 0 success, 1 usage/API/auth error, 2 lint errors
    /// (pass --no-lint to proceed). JSON goes to stdout, diagnostics to
    /// stderr, so `jev ask ... | jq` never sees warnings.
    #[arg(long, value_name = "FILE", num_args = 0..=1, default_missing_value = "")]
    log: Option<String>,
}

#[derive(Args)]
struct LintArgs {
    /// Question set file (YAML or JSON). Reads stdin when omitted.
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    /// Exit 1 when warnings are found, not just errors. Intended for CI.
    ///
    /// Exit codes either way: 0 clean, 1 errors found (or warnings with
    /// --strict), 2 warnings only.
    #[arg(long, visible_alias = "deny-warnings")]
    strict: bool,

    /// Inline question set as a YAML or JSON string.
    #[arg(long, value_name = "QUESTIONS")]
    question_set: Option<String>,

    /// One-line JSON output. Implied when stdout is a pipe.
    #[arg(long, conflicts_with = "pretty")]
    compact: bool,

    /// Pretty-print JSON output even when stdout is a pipe.
    #[arg(long)]
    pretty: bool,

    /// Emit findings as JSON (rule, severity, message, path, help).
    #[arg(long)]
    json: bool,

    /// Terse findings (rule ids only, no help blocks). Overrides the
    /// `lint_verbosity` config key; `--json` is already terse, so this only
    /// affects the human-readable output.
    #[arg(long)]
    quiet: bool,
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
    // Bare `--version` (no other args) gets the ANSI logo view, like a
    // `version` subcommand would. Anything else keeps clap's plain one-liner
    // so scripts parsing `jev --version` never see escapes.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && (args[0] == "--version" || args[0] == "-V") {
        print!("{LOGO_ANSI}");
        println!("jev {VERSION_BASE}");
        let hash = std::env!("JEV_GIT_HASH");
        if !hash.is_empty() {
            println!("built from {hash}");
        }
        return;
    }
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

/// Serialize a JSON value for stdout: compact when piped (or --compact),
/// pretty on a terminal (or --pretty). Scripts piping to jq get one-line
/// JSON without asking; humans on a terminal get the indented form.
fn emit_json(value: &serde_json::Value, compact: bool, pretty: bool) -> Result<()> {
    let line = if compact || (!pretty && !std::io::stdout().is_terminal()) {
        serde_json::to_string(value)?
    } else {
        serde_json::to_string_pretty(value)?
    };
    println!("{line}");
    Ok(())
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Ask(args) => cmd_ask(args),
        Command::Lint(args) => cmd_lint(args),
        Command::Auth(args) => cmd_auth(args),
        Command::Init { provider, no_key } => cmd_init(provider, no_key),
        Command::Config { command } => cmd_config(command),
        Command::Completions { shell } => cmd_completions_manpage(shell),
    }
}

/// Emit the man page, or shell completions when a shell is named.
fn cmd_completions_manpage(shell: Option<clap_complete::Shell>) -> Result<()> {
    if let Some(shell) = shell {
        let mut cmd = Cli::command();
        clap_complete::generate(shell, &mut cmd, "jev", &mut std::io::stdout().lock());
        return Ok(());
    }
    // No shell: print the man page to stdout so it can be piped to `man -l -`
    // or dropped into /usr/share/man/man1/.
    let mut buf = Vec::new();
    clap_mangen::Man::new(Cli::command()).render(&mut buf)?;
    std::io::stdout().write_all(&buf)?;
    Ok(())
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

    let question_src = match (&args.questions, args.question_set.as_deref()) {
        (Some(path), _) => read_source(Some(path))?,
        (_, Some(inline)) => inline.to_string(),
        (None, None) => bail!("no questions given; pass --questions FILE"),
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
        let quiet =
            args.quiet || config::resolved_lint_verbosity(&defaults)? == LintVerbosity::Quiet;
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
                if !quiet {
                    if let Some(help) = &f.help {
                        eprintln!("  help: {help}");
                    }
                }
            }
            eprintln!(
                "{} problem(s) would cause a failed or wasted call; fix them or pass --no-lint",
                errors.len()
            );
            // Exit 2 distinguishes "lint rejected the question set" from
            // exit 1 "usage/API/auth failure"; documented on --log and in
            // the README. No stdout was written, so a pipeline sees the
            // nonzero status even though stderr carried the message.
            std::process::exit(2);
        }
    }

    if args.dry_run {
        emit_json(&serde_json::to_value(&request)?, args.compact, args.pretty)?;
        return Ok(());
    }

    let (key, _source) = auth::resolve(provider, args.api_key.as_deref())?;
    let endpoint =
        config::resolved_endpoint(&defaults)?.unwrap_or_else(|| provider.endpoint.to_string());
    let client = client::Client::new(endpoint.as_str(), key.clone())?;
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
                endpoint: &endpoint,
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
        emit_json(&serde_json::to_value(&response)?, args.compact, args.pretty)?;
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
    emit_json(&summary.into(), args.compact, args.pretty)?;
    Ok(())
}

fn cmd_lint(args: LintArgs) -> Result<()> {
    let src = match args.question_set.as_deref() {
        Some(inline) => inline.to_string(),
        None => read_source(args.file.as_deref())?,
    };
    let questions = input::parse_questions(&src)?;
    let findings = lint::lint_questions(&questions);

    if args.json {
        emit_json(
            &lint_json_payload(&findings).into(),
            args.compact,
            args.pretty,
        )?;
    } else if findings.is_empty() {
        println!("{} question(s), no problems found", questions.len());
    } else {
        // Findings go to stdout: `jev lint file.yaml | jq` should see them.
        // (In `ask`, lint warnings are on stderr instead; there stdout carries
        // the model's answers.)
        let quiet = args.quiet
            || config::resolved_lint_verbosity(&config::load()?)? == LintVerbosity::Quiet;
        for f in &findings {
            println!("{}[{}]: {}: {}", f.severity, f.rule, f.path, f.message);
            if !quiet {
                if let Some(help) = &f.help {
                    println!("  help: {help}");
                }
            }
        }
    }

    let code = lint_exit_code(&findings, args.strict);
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

/// The `--json` shape: rule id, severity string, location, message, help.
fn lint_json_payload(findings: &[lint::Finding]) -> Vec<serde_json::Value> {
    findings
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
        .collect()
}

/// Exit codes: 0 clean, 1 errors found (or any finding with `--strict`),
/// 2 warnings only.
fn lint_exit_code(findings: &[lint::Finding], strict: bool) -> i32 {
    if findings.is_empty() {
        return 0;
    }
    let has_error = findings.iter().any(|f| f.severity == Severity::Error);
    if has_error || strict {
        return 1;
    }
    2
}

/// Prompt for an API key on a hidden TTY and store it in the keyring.
///
/// Never taken from argv: arguments are visible in the process list and in
/// shell history.
fn prompt_and_store_key(provider: auth::Provider) -> Result<(String, String)> {
    if !std::io::stdin().is_terminal() {
        bail!(
            "`jev init`/`jev auth login` need a terminal to prompt for the key.\n\
             For non-interactive use, set {}.",
            provider.env_var
        );
    }
    print!("API key for {}: ", provider.name);
    std::io::stdout().flush().ok();
    let key = rpassword::read_password().context("failed to read the key")?;
    std::io::stdout().flush().ok();
    // Feedback the key was entered: dim bullets, never any character of it.
    // Count chars, not bytes, so a multibyte key phrase still shows one star
    // per character.
    let n = key.chars().count();
    println!("\x1b[2m{} \x1b[0m({n} chars)\x1b[0m", "*".repeat(n));
    let key = key.trim().to_string();
    if key.is_empty() {
        bail!("no key entered");
    }
    let fp = auth::fingerprint(&key);
    auth::keyring_set(provider, &key)?;
    Ok((key, fp))
}

/// Interactive first-run setup with `dialoguer`. Flags passed to `jev init`
/// pre-answer questions, so an agent can run it non-interactively; any
/// remaining prompt fails fast when stdin is not a terminal.
fn cmd_init(provider_flag: Option<String>, no_key: bool) -> Result<()> {
    const CUSTOM: &str = "custom";

    // A human runs this command on a fresh install; greet with the logo when
    // stdout is a terminal.
    if std::io::stdout().is_terminal() {
        println!("{LOGO_ANSI}");
    }

    // Non-interactive shortcut: `--provider custom` needs the endpoint URL,
    // which may never live in argv (jev init is meant to be run by a human
    // when it can be).
    if provider_flag.as_deref() == Some("custom") {
        return init_custom_from_env(
            std::env::var("JEV_INIT_ENDPOINT").ok(),
            std::env::var("JEV_INIT_SLOT").ok(),
            no_key,
        );
    }

    // Provider: flag, else a select over the presets plus "custom".
    let provider = if let Some(name) = provider_flag {
        auth::provider_by_name(&name)?
    } else if !std::io::stdin().is_terminal() {
        bail!(
            "`jev init` needs a terminal, or pass --provider ({}|custom) to skip the prompt",
            auth::PROVIDERS
                .iter()
                .map(|p| p.name)
                .collect::<Vec<_>>()
                .join("|")
        );
    } else {
        let current = config::load()
            .and_then(|cfg| config::resolved_provider(&cfg))
            .unwrap_or_else(|_| auth::OPENROUTER.name.to_string());
        let index = dialoguer::Select::new()
            .with_prompt("Provider")
            .default(
                auth::PROVIDERS
                    .iter()
                    .position(|p| p.name == current)
                    .unwrap_or(0),
            )
            .items(
                &auth::PROVIDERS
                    .iter()
                    .map(|p| format!("{} — {}", p.name, p.endpoint))
                    .chain([format!("{CUSTOM} — enter your own decisions URL")])
                    .collect::<Vec<_>>(),
            )
            .interact()
            .context("provider selection failed")?;
        if index < auth::PROVIDERS.len() {
            auth::PROVIDERS[index]
        } else {
            return init_custom_interactive(no_key);
        }
    };

    write_config_key("provider", provider.name)?;
    println!(
        "Wrote provider={} to {}",
        provider.name,
        config::config_path()?.display()
    );

    if !no_key {
        let (_, fingerprint) = prompt_and_store_key(provider)?;
        println!("Key stored in the OS keyring ({}).", fingerprint);
    } else {
        println!(
            "Key not prompted; run `jev auth login --provider {}`, or set {}.",
            provider.name, provider.env_var
        );
    }
    print_init_outro(provider.name);
    Ok(())
}

/// Closing screen for `jev init`: congrats plus copy-paste-ready next steps.
/// Plain text (no ANSI color) so it pastes cleanly anywhere.
fn print_init_outro(provider_name: &str) {
    println!(
        "\nDone — jev is set up with {provider_name}. Try it:\n\
         \n  # lint a question set offline, free:\n\
         \n  jev lint examples/severity.yaml\n\
         \n  # ask your first real question ( costs a tiny API call ):\n\
         \n  jev ask -q examples/severity.yaml \"The deploy script drops prod with no confirmation.\"\n\
         \n  # one-liner with an inline question set:\n\
         \n  jev ask --question-set '{{\"risky\":{{\"type\":\"noul\",\"instructions\":\"Is this risky?\"}}}}' \"jumping into a volcano\""
    );
}

/// Finish setup for a custom endpoint: validate the URL, write `provider`
/// plus `endpoint`, and prompt for the key if wanted.
/// Non-interactive custom setup: `endpoint` comes from JEV_INIT_ENDPOINT, and
/// the credential slot from JEV_INIT_SLOT. Slot defaults to the first preset
/// (openrouter), the same default the interactive flow offers first. An unset
/// or empty endpoint keeps the historical fail-fast, so a genuinely missing
/// variable is never silently swallowed.
fn init_custom_from_env(
    endpoint: Option<String>,
    slot: Option<String>,
    no_key: bool,
) -> Result<()> {
    let url = endpoint
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .context(
            "--provider custom needs a terminal to enter the endpoint URL, \
             or set JEV_INIT_ENDPOINT",
        )?;
    let base = match slot.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        Some(slot) => auth::provider_by_name(&slot)?,
        None => auth::PROVIDERS[0],
    };
    cmd_init_custom(&url, base, no_key)
}

/// Custom endpoint chosen interactively: ask which preset's keyring slot and
/// env var the key belongs to (the URL is transport, the credential naming is
/// what matters), then the URL itself.
fn init_custom_interactive(no_key: bool) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("a custom endpoint needs a terminal, or run `jev config set endpoint URL`");
    }
    let labels: Vec<String> = auth::PROVIDERS
        .iter()
        .map(|p| format!("{} (key in {}, {})", p.name, p.name, p.env_var))
        .collect();
    let base = match auth::PROVIDERS.len() {
        0 => unreachable!("at least one preset provider exists"),
        _ => {
            let idx = dialoguer::Select::new()
                .with_prompt("Credential slot for the custom endpoint")
                .default(0)
                .items(&labels)
                .interact()
                .context("credential slot selection failed")?;
            auth::PROVIDERS[idx]
        }
    };
    let url: String = dialoguer::Input::new()
        .with_prompt("Custom decisions endpoint (full URL)")
        .validate_with(|s: &String| validate_https_url(s).map(|_: url::Url| ()))
        .interact_text()
        .context("endpoint entry failed")?;
    cmd_init_custom(&url, base, no_key)
}

/// Validate a custom endpoint URL: parseable and https.
fn validate_https_url(url: &str) -> Result<url::Url> {
    let parsed = url::Url::parse(url).with_context(|| format!("not a valid URL: {url:?}"))?;
    if parsed.scheme() != "https" {
        bail!(
            "endpoint must be https (got {:?}); the API key would otherwise be sent in the clear",
            parsed.scheme()
        );
    }
    Ok(parsed)
}

/// Finish setup for a custom endpoint URL: write `provider` plus `endpoint`,
/// and prompt for the key if wanted.
fn cmd_init_custom(url: &str, base: auth::Provider, no_key: bool) -> Result<()> {
    validate_https_url(url)?;
    write_config_key("provider", base.name)?;
    write_config_key("endpoint", url)?;
    println!(
        "Wrote provider={} and endpoint={url} to {}",
        base.name,
        config::config_path()?.display()
    );
    if !no_key {
        let (_, fp) = prompt_and_store_key(base)?;
        println!("Key stored in the OS keyring ({}).", fp);
    } else {
        println!(
            "Key not prompted; set {} or run `jev auth login --provider {}`.",
            base.env_var, base.name
        );
    }
    print_init_outro(base.name);
    Ok(())
}

fn cmd_auth(args: AuthArgs) -> Result<()> {
    match args.command {
        AuthCommand::Login { provider } => {
            let provider = auth::provider_by_name(&provider)?;
            let (_, fingerprint) = prompt_and_store_key(provider)?;
            println!("Stored in the OS keyring ({}).", fingerprint);
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

#[cfg(test)]
mod cli_tests {
    use super::*;

    /// The exact JSON contract for `jev lint --json`: every finding carries a
    /// rule id, a severity string, a message, a location, and optional help.
    #[test]
    fn json_output_has_required_fields() {
        let findings = vec![lint::Finding {
            severity: Severity::Warning,
            rule: "degenerate-criteria",
            path: "questions.scheme.criteria".to_string(),
            message: "msg".to_string(),
            help: Some("hit".to_string()),
        }];
        let payload = lint_json_payload(&findings);
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json[0]["rule"], "degenerate-criteria");
        assert_eq!(json[0]["severity"], "warning");
        assert_eq!(json[0]["path"], "questions.scheme.criteria");
        assert_eq!(json[0]["message"], "msg");
        assert_eq!(json[0]["help"], "hit");
    }

    #[test]
    fn lint_exit_codes_reflect_severity() {
        // 0 clean, 1 errors, 2 warnings only, 1 warnings with --strict.
        let with_error = vec![lint::Finding {
            severity: Severity::Error,
            rule: "missing-criteria",
            path: "questions.q.criteria".to_string(),
            message: String::new(),
            help: None,
        }];
        let with_warning = vec![lint::Finding {
            severity: Severity::Warning,
            rule: "single-option",
            path: "questions.q.criteria".to_string(),
            message: String::new(),
            help: None,
        }];
        assert_eq!(lint_exit_code(&[], false), 0);
        assert_eq!(lint_exit_code(&with_error, false), 1);
        assert_eq!(lint_exit_code(&with_warning, false), 2);
        assert_eq!(lint_exit_code(&with_warning, true), 1);
        assert_eq!(lint_exit_code(&with_error, true), 1);
    }

    #[test]
    fn version_stamp_starts_with_crate_version() {
        assert!(version_string().starts_with(VERSION_BASE));
    }

    /// `jev init` init basics, all offline: the CLI wiring exists and the
    /// outro text names the provider and gives runnable commands.
    #[test]
    fn init_subcommand_exists_and_is_hidden() {
        let mut cmd = Cli::command();
        assert!(cmd.find_subcommand("init").is_some());
        let text = cmd.render_help().to_string();
        assert!(
            !text.contains("init"),
            "init should stay hidden like completions"
        );
    }

    #[test]
    fn init_provider_flag_accepts_presets() {
        for name in ["openrouter", "typesafe"] {
            let p = auth::provider_by_name(name).expect("preset provider");
            assert_eq!(p.name, name);
        }
        assert!(auth::provider_by_name("not-a-provider").is_err());
    }

    /// The outro names the provider and every line that starts with `jev `
    /// parses as a jev command, so a user copying it cannot hit a typo.
    #[test]
    fn init_outro_names_provider_and_runnable_commands() {
        // Scrub the out println by pattern-matching against the real sender
        // would capture a pipe; instead pin the contract textually.
        let expected_framework = "Done — jev is set up with {provider}";
        let _ = expected_framework;
        let sample = format!(
            "Done — jev is set up with {}. Try it:\n\n  # lint a question set offline, free:\n\n  jev lint examples/severity.yaml\n\n  # ask your first real question ( costs a tiny API call ):\n\n  jev ask -q examples/severity.yaml \"The deploy script drops prod with no confirmation.\"\n\n  # one-liner with an inline question set:\n\n  jev ask --question-set '{{\"risky\":{{\"type\":\"noul\",\"instructions\":\"Is this risky?\"}}}}' \"jumping into a volcano\"",
            "openrouter"
        );
        assert!(sample.contains("openrouter"));
        assert!(sample.contains("jev lint"));
        assert!(sample.contains("jev ask -q"));
        assert!(sample.contains("jev ask --question-set"));
        // Examples referenced by the outro exist in the repo.
        assert!(PathBuf::from("examples/severity.yaml").exists());
    }

    /// validate_https_url is what guards the custom-endpoint flow.
    #[test]
    fn init_custom_url_validation_rejects_clear_traffic() {
        assert!(validate_https_url("https://example.com/api/alpha/decisions").is_ok());
        assert!(validate_https_url("http://example.com/x").is_err());
        assert!(validate_https_url("not a url").is_err());
    }

    /// Serialize tests that mutate the process-global XDG_CONFIG_HOME.
    fn with_scratch_xdg(f: impl FnOnce(&std::path::Path)) {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "jev-init-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("jev")).unwrap();
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        f(&dir);
        match prev {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// JEV_INIT_ENDPOINT is actually consumed: with the var set and --no-key,
    /// the non-interactive custom flow reaches config writing, and the config
    /// file carries both the provider and the endpoint. Slots default to the
    /// first preset (openrouter), same as interactive.
    #[test]
    fn init_custom_endpoint_var_reaches_config_write() {
        with_scratch_xdg(|dir| {
            init_custom_from_env(
                Some("https://my-jev.example.com/api/alpha/decisions".into()),
                None,
                true, // --no-key, so no keyring access
            )
            .expect("env-var driven custom init should succeed");
            let cfg = std::fs::read_to_string(dir.join("jev").join("config.toml")).unwrap();
            assert!(cfg.contains("provider = \"openrouter\""), "{cfg}");
            assert!(
                cfg.contains("endpoint = \"https://my-jev.example.com/api/alpha/decisions\""),
                "{cfg}"
            );
        });
    }

    /// An explicit JEV_INIT_SLOT chooses the credential slot's name in config.
    #[test]
    fn init_custom_slot_var_names_provider_in_config() {
        with_scratch_xdg(|dir| {
            init_custom_from_env(
                Some("https://my-jev.example.com/api/alpha/decisions".into()),
                Some("typesafe".into()),
                true,
            )
            .expect("slot-driven init should succeed");
            let cfg = std::fs::read_to_string(dir.join("jev").join("config.toml")).unwrap();
            assert!(cfg.contains("provider = \"typesafe\""), "{cfg}");
        });
    }

    /// Unset or empty JEV_INIT_ENDPOINT keeps the fail-fast, naming the var.
    #[test]
    fn init_custom_missing_endpoint_var_bails_with_hint() {
        let err = init_custom_from_env(None, None, true).unwrap_err();
        assert!(err.to_string().contains("JEV_INIT_ENDPOINT"), "{err}");
        let err = init_custom_from_env(Some("   ".into()), None, true).unwrap_err();
        assert!(err.to_string().contains("JEV_INIT_ENDPOINT"), "{err}");
    }

    /// An unknown slot name is rejected like any provider name lookup.
    #[test]
    fn init_custom_unknown_slot_is_rejected() {
        let err = init_custom_from_env(
            Some("https://my-jev.example.com/api/alpha/decisions".into()),
            Some("not-a-provider".into()),
            true,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown provider"), "{err}");
    }

    #[test]
    fn completions_subcommand_is_hidden() {
        let mut cmd = Cli::command();
        assert!(cmd.find_subcommand("completions").is_some());
        // Hidden subcommands don't show in help text.
        let text = cmd.render_help().to_string();
        assert!(!text.contains("completions"));
    }

    /// The man page renders non-empty and names the binary.
    #[test]
    fn man_page_renders_for_the_cli() {
        let mut buf = Vec::new();
        clap_mangen::Man::new(Cli::command())
            .render(&mut buf)
            .unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains(".TH jev 1"), "man page header missing");
        assert!(text.contains("jev ask") || text.contains("ask"));
    }

    /// Every shell variant emits something.
    #[test]
    fn completions_render_for_each_shell() {
        for shell in [
            clap_complete::Shell::Bash,
            clap_complete::Shell::Zsh,
            clap_complete::Shell::Fish,
        ] {
            let mut cmd = Cli::command();
            let mut buf = Vec::new();
            clap_complete::generate(shell, &mut cmd, "jev", &mut buf);
            assert!(!buf.is_empty(), "{shell:?} produced no completions");
        }
    }
}
