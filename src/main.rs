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
mod input;
mod lint;
mod types;

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
    /// Manage stored API credentials
    Auth(AuthArgs),
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
    }
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

    let provider = auth::provider_by_name(&args.provider)?;
    let model = args
        .model
        .unwrap_or_else(|| provider.default_model.to_string());

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
    let response = client.decide(&request)?;

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
