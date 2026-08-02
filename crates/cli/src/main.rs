use std::collections::BTreeMap;
use std::io::Write as _;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, BufReader};
use zest_core::{
    detect_all, AuthStatus, Config, Ledger, ProviderConfig, Routing, RuntimeBuilder, StreamEvent,
    Target, Thread, ThreadStore, DEFAULT_MODEL,
};

const SYSTEM: &str = "\
You are Zest, a coding agent running in a terminal inside the user's project. You \
have project tools (list_dir, glob, grep, read_file, write_file) scoped to that \
project. Explore and read files before answering questions about them rather than \
inferring from names. write_file requires approval; the CLI currently auto-denies \
writes (use the desktop app to allow them). Keep responses focused and concise.";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    match std::env::args().nth(1).as_deref() {
        // Terminal form of the launch picker.
        Some("auth") => {
            print_auth();
            return Ok(());
        }
        Some("usage") => {
            print_usage();
            return Ok(());
        }
        Some("doctor") => {
            let live = std::env::args().skip(2).any(|a| a == "--live");
            if !live {
                print_doctor_help();
                std::process::exit(2);
            }
            run_doctor_live().await?;
            return Ok(());
        }
        _ => {}
    }

    let root = std::env::current_dir()?;
    let effort = std::env::var("ZEST_EFFORT").unwrap_or_else(|_| "high".to_string());

    // ZEST_BASE_URL remains a one-off override for pointing at a gateway without
    // writing config. It builds the same single-gateway shape zest.toml would, so
    // there is only ever one code path from here down.
    let config = match gateway_override() {
        Some(config) => config,
        None => Config::find(&root)?,
    };

    for issue in config.lint() {
        eprintln!("\x1b[33mwarning:\x1b[0m {issue}");
    }

    let runtime = RuntimeBuilder::new(&root)
        .with_config(config)
        .with_effort(effort)
        .with_system(SYSTEM)
        .enable_delegate(true)
        .register_write_tools(true)
        .build()?;

    let mut agent = runtime.agent;

    println!(
        "zest — {} · {} · root {}",
        agent.model,
        runtime.provider_id,
        root.display()
    );
    if runtime.registry.len() > 1 {
        let others: Vec<_> = runtime
            .registry
            .ids()
            .filter(|id| *id != runtime.provider_id)
            .collect();
        println!("also configured: {}", others.join(", "));
        println!("delegate: enabled (multi-provider workers)");
    }
    println!("tools: list_dir, glob, grep, read_file, write_file");
    println!("note: write_file is gated; CLI auto-denies writes (desktop can Allow once)");
    println!("ctrl-c to quit\n");

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        print!("\x1b[1m>\x1b[0m ");
        std::io::stdout().flush()?;

        let Some(line) = lines.next_line().await? else {
            break; // EOF
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut render = Renderer::default();
        let mut on_event = |ev: StreamEvent<'_>| render.handle(ev);

        if let Err(e) = agent.send(line, &mut on_event).await {
            eprintln!("\n\x1b[31merror:\x1b[0m {e}");
        }
        println!("\n");
    }

    Ok(())
}

fn print_doctor_help() {
    eprintln!(
        "\
zest doctor --live

Opt-in live acceptance check for Stable Windows Alpha. Spends real quota.

Runs one read-only tool turn against README.md in the current directory and
verifies streaming events, tool completion, usage-ledger delta, and thread
persistence. Write tools and delegation are disabled for this command.

Requires a working provider (see zest.toml / ZEST_GATEWAY_KEY) and a README.md
in the workspace root.

This is manual on purpose — do not wire it into CI.
"
    );
}

/// One real Messages-API turn: read README.md, assert stream/tool/usage/persist.
async fn run_doctor_live() -> anyhow::Result<()> {
    let root = std::env::current_dir()?;
    let readme = root.join("README.md");
    if !readme.is_file() {
        anyhow::bail!("doctor --live needs README.md in {}", root.display());
    }

    println!("zest doctor --live");
    println!("workspace: {}", root.display());
    println!("note: spends quota; read-only tools only\n");

    let config = match gateway_override() {
        Some(config) => config,
        None => Config::find(&root)?,
    };
    for issue in config.lint() {
        eprintln!("\x1b[33mwarning:\x1b[0m {issue}");
    }

    // Isolated ledger file so doctor does not mix with the global usage book.
    let ledger_path = root.join(".zest").join("doctor-usage.json");
    let _ = std::fs::remove_file(&ledger_path);
    let ledger = Arc::new(Mutex::new(Ledger::load_from(&ledger_path)));
    let before_requests = 0u64;

    let runtime = RuntimeBuilder::new(&root)
        .with_config(config)
        .with_system(
            "You are running zest doctor --live. Call read_file on README.md \
             (path exactly README.md), then reply with one short sentence that \
             includes the word zest. Do not write files or call other tools.",
        )
        .with_ledger(ledger.clone())
        .enable_delegate(false)
        .register_write_tools(false)
        .build()?;

    println!(
        "provider {} · model {} · effort {}",
        runtime.provider_id, runtime.model, runtime.effort
    );

    let mut agent = runtime.agent;
    let mut saw_text = false;
    let mut saw_tool_start = false;
    let mut saw_tool_ok = false;
    let mut tool_error: Option<String> = None;

    let mut on_event = |ev: StreamEvent<'_>| match ev {
        StreamEvent::Text(t) => {
            if !t.is_empty() {
                saw_text = true;
                print!("{t}");
                let _ = std::io::stdout().flush();
            }
        }
        StreamEvent::Thinking(t) => {
            if !t.is_empty() {
                print!("\x1b[90m{t}\x1b[0m");
                let _ = std::io::stdout().flush();
            }
        }
        StreamEvent::ToolCallStart { name, .. } => {
            println!("\n→ {name}");
            if name == "read_file" {
                saw_tool_start = true;
            }
        }
        StreamEvent::ToolCallResult {
            name,
            summary,
            is_error,
            ..
        } => {
            if is_error {
                println!("✗ {name} {summary}");
                tool_error = Some(format!("{name}: {summary}"));
            } else {
                println!("✓ {name}");
                if name == "read_file" {
                    saw_tool_ok = true;
                }
            }
        }
        StreamEvent::ApprovalNeeded { tool_name, .. } => {
            tool_error = Some(format!("unexpected approval for {tool_name}"));
        }
    };

    agent
        .send(
            "Read README.md with the read_file tool, then confirm briefly.",
            &mut on_event,
        )
        .await?;
    println!("\n");

    if let Some(err) = tool_error {
        anyhow::bail!("doctor tool failure: {err}");
    }
    if !saw_tool_start {
        anyhow::bail!("doctor failed: model never started read_file");
    }
    if !saw_tool_ok {
        anyhow::bail!("doctor failed: read_file did not complete successfully");
    }
    if !saw_text {
        anyhow::bail!("doctor failed: no streamed text deltas");
    }

    let provider_id = agent.provider_id().to_string();
    let after = {
        let guard = ledger.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
        guard
            .get(&provider_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("doctor failed: no ledger entry for `{provider_id}`"))?
    };
    if after.requests <= before_requests {
        anyhow::bail!(
            "doctor failed: usage did not increase (before={before_requests}, after={})",
            after.requests
        );
    }

    // Persist + restore the wire history the way a session reopen would.
    let store = ThreadStore::open(&root)?;
    let mut thread = Thread::new().with_provider(&provider_id);
    thread.title = Some("doctor --live".into());
    thread.agent_messages = agent.messages.clone();
    store.save(&thread)?;
    let loaded = store.load_with_recovery(&thread.id)?;
    if loaded.thread.agent_messages.len() < 2 {
        anyhow::bail!("doctor failed: persisted thread missing wire history");
    }
    if loaded.thread.provider_id.as_deref() != Some(provider_id.as_str()) {
        anyhow::bail!("doctor failed: provider_id not restored");
    }

    println!("checks:");
    println!("  streaming text ........ ok");
    println!("  read_file tool ........ ok");
    println!(
        "  usage delta ........... ok ({} → {} req on {provider_id})",
        before_requests, after.requests
    );
    println!(
        "  persistence ........... ok (thread {})",
        loaded.thread.id
    );
    println!("\n\x1b[32mdoctor --live passed\x1b[0m");
    Ok(())
}

fn print_auth() {
    println!("\n\x1b[1mProviders\x1b[0m\n");

    for slot in detect_all() {
        let (mark, detail) = match &slot.status {
            AuthStatus::Ready { account } => (
                "\x1b[32m●\x1b[0m",
                account.clone().unwrap_or_else(|| "signed in".into()),
            ),
            // Deliberately not red: we cannot see the credentials, which is not
            // the same as their being absent.
            AuthStatus::Unknown { reason } => ("\x1b[33m●\x1b[0m", reason.clone()),
            AuthStatus::NotLoggedIn { fix } => ("\x1b[90m○\x1b[0m", format!("run: {fix}")),
            AuthStatus::Unconfigured => ("\x1b[90m○\x1b[0m", "no key set".into()),
        };

        println!(
            "  {mark} \x1b[1m{:<13}\x1b[0m \x1b[90m{:<20}\x1b[0m {detail}",
            slot.label, slot.method
        );
    }

    println!("\n\x1b[90m● selectable   ○ unavailable\x1b[0m\n");
}

/// `ZEST_BASE_URL` as a synthetic single-gateway config.
///
/// Pointing it at Anthropic's own host is a no-op — that is just the default
/// provider, so fall through to the real config instead.
fn gateway_override() -> Option<Config> {
    let base = std::env::var("ZEST_BASE_URL").ok()?;
    let base = base.trim();
    if base.is_empty() || base.contains("api.anthropic.com") {
        return None;
    }

    let model = std::env::var("ZEST_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    // Prefer the gateway client token; fall back to ANTHROPIC_API_KEY for the
    // Claude-Code-shaped env that many proxy writeups still document.
    let api_key_env = if std::env::var("ZEST_GATEWAY_KEY")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        "ZEST_GATEWAY_KEY"
    } else {
        "ANTHROPIC_API_KEY"
    };
    let mut providers = BTreeMap::new();
    providers.insert(
        "gateway".to_string(),
        ProviderConfig::Gateway {
            base_url: base.to_string(),
            api_key_env: Some(api_key_env.to_string()),
            model,
            models: Vec::new(),
            efforts: Vec::new(),
        },
    );

    Some(Config {
        providers,
        routing: Routing {
            default: Some(Target {
                provider: "gateway".to_string(),
                model: None,
            }),
            rules: Vec::new(),
        },
    })
}

/// Spend and headroom are printed as separate lines on purpose. They answer
/// different questions and one of them is not ours to measure.
fn print_usage() {
    let ledger = Ledger::load();

    println!("\n\x1b[1mUsage\x1b[0m");
    if let Some(path) = ledger.path() {
        println!("\x1b[90m{}\x1b[0m", path.display());
    }
    println!();

    if ledger.is_empty() {
        println!("  \x1b[90mNothing recorded yet.\x1b[0m\n");
        return;
    }

    for (id, usage) in ledger.entries() {
        println!("  \x1b[1m{id}\x1b[0m");
        println!(
            "    spent      {} req · {} in · {} out  \x1b[90m(measured by Zest)\x1b[0m",
            usage.requests,
            compact(usage.input_tokens),
            compact(usage.output_tokens),
        );
        match &usage.headroom {
            Some(h) if !h.is_empty() => {
                let req = h
                    .requests_remaining
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "?".into());
                println!(
                    "    headroom   {req} req remaining  \x1b[90m(provider-reported throughput)\x1b[0m"
                );
            }
            _ => {
                println!("    headroom   \x1b[90mnot reported by provider\x1b[0m");
            }
        }
        println!();
    }
}

fn compact(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[derive(Default)]
struct Renderer {
    thinking_open: bool,
    text_started: bool,
}

impl Renderer {
    fn handle(&mut self, ev: StreamEvent<'_>) {
        match ev {
            StreamEvent::Thinking(t) => {
                if !self.thinking_open {
                    print!("\x1b[90m");
                    self.thinking_open = true;
                }
                print!("{t}");
                let _ = std::io::stdout().flush();
            }
            StreamEvent::Text(t) => {
                if self.thinking_open {
                    print!("\x1b[0m\n");
                    self.thinking_open = false;
                }
                if !self.text_started {
                    self.text_started = true;
                }
                print!("{t}");
                let _ = std::io::stdout().flush();
            }
            StreamEvent::ToolCallStart { name, .. } => {
                if self.thinking_open {
                    print!("\x1b[0m\n");
                    self.thinking_open = false;
                }
                println!("\n\x1b[36m→ {name}\x1b[0m");
            }
            StreamEvent::ToolCallResult {
                name,
                summary,
                is_error,
                ..
            } => {
                if is_error {
                    println!("\x1b[31m✗ {name}\x1b[0m \x1b[90m{summary}\x1b[0m");
                } else {
                    println!("\x1b[32m✓ {name}\x1b[0m \x1b[90m{summary}\x1b[0m");
                }
            }
            StreamEvent::ApprovalNeeded {
                tool_name,
                summary,
                ..
            } => {
                println!("\n\x1b[33m? approve {tool_name}\x1b[0m \x1b[90m{summary}\x1b[0m");
            }
        }
    }
}
