use std::collections::BTreeMap;
use std::io::Write as _;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, BufReader};
use zest_core::{
    detect_all, Agent, AuthStatus, Config, Delegate, Ledger, ProviderConfig, ProviderRegistry,
    ReadFile, Router, Routing, StreamEvent, Target, ToolRegistry, DEFAULT_MODEL,
};

const SYSTEM: &str = "\
You are Zest, a coding agent running in a terminal inside the user's project. You \
have a read_file tool scoped to that project. Read files before answering questions \
about them rather than inferring from names. Keep responses focused and concise.";

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

    let (registry, skipped) = ProviderRegistry::from_config(&config);
    for entry in &skipped {
        eprintln!(
            "\x1b[33mwarning:\x1b[0m provider `{}` unavailable — {}",
            entry.id, entry.reason
        );
    }

    let target = config.default_target().ok_or_else(|| {
        anyhow::anyhow!(
            "no default provider. With more than one configured, zest.toml needs:\n  \
             [routing]\n  default = {{ provider = \"...\" }}"
        )
    })?;

    let provider = registry.get(&target.provider).ok_or_else(|| {
        anyhow::anyhow!(
            "provider `{}` is configured but could not be loaded — see the warnings above",
            target.provider
        )
    })?;

    // Most specific wins: the routing target, then the environment, then whatever
    // the provider considers its own default.
    let model = target
        .model
        .clone()
        .or_else(|| std::env::var("ZEST_MODEL").ok())
        .unwrap_or_else(|| provider.default_model().to_string());

    let ledger = Arc::new(Mutex::new(Ledger::load()));

    // What a delegated worker can do. Deliberately never includes `delegate`
    // itself — that is enforced in Delegate::new.
    let mut worker_tools = ToolRegistry::new();
    worker_tools.register(Arc::new(ReadFile::new(&root)?));

    let mut tools = worker_tools.clone();

    // Delegation only earns its place in the prompt when there is somewhere else
    // to send work.
    let registry = Arc::new(registry);
    if registry.len() > 1 {
        let mut kinds: Vec<String> = config.routing.rules.iter().map(|r| r.kind.clone()).collect();
        kinds.sort();
        kinds.dedup();

        tools.register(Arc::new(
            Delegate::new(
                registry.clone(),
                Arc::new(Router::from_config(&config)),
                worker_tools,
            )
            .with_ledger(ledger.clone())
            .with_kinds(kinds),
        ));
    }

    let tool_names = tools.names().join(", ");

    let mut agent = Agent::new(provider, tools)
        .with_system(SYSTEM)
        .with_ledger(ledger);
    agent.model = model;
    agent.effort = effort;

    println!(
        "zest — {} · {} · root {}",
        agent.model,
        target.provider,
        root.display()
    );
    if registry.len() > 1 {
        let others: Vec<_> = registry.ids().filter(|id| *id != target.provider).collect();
        println!("also configured: {}", others.join(", "));
    }
    println!("tools: {tool_names}");
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
    let mut providers = BTreeMap::new();
    providers.insert(
        "gateway".to_string(),
        ProviderConfig::Gateway {
            base_url: base.to_string(),
            api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
            model,
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
            Some(h) => {
                let mut parts = Vec::new();
                if let Some(v) = h.requests_remaining {
                    parts.push(format!("{} req", compact(v)));
                }
                if let Some(v) = h.input_tokens_remaining {
                    parts.push(format!("{} in", compact(v)));
                }
                if let Some(v) = h.output_tokens_remaining {
                    parts.push(format!("{} out", compact(v)));
                }
                let age = usage
                    .headroom_at
                    .map(|t| format!(", {}", ago(t)))
                    .unwrap_or_default();
                println!(
                    "    headroom   {}  \x1b[90m(reported by provider{age})\x1b[0m",
                    if parts.is_empty() {
                        "—".to_string()
                    } else {
                        parts.join(" · ")
                    },
                );
            }
            None => println!("    headroom   \x1b[90mnot reported by this provider\x1b[0m"),
        }
        println!();
    }

    println!(
        "\x1b[90m  spent    = this machine only; other clients on the same account are invisible\n  \
         headroom = short-window throughput from the provider, not plan quota\x1b[0m\n"
    );
}

fn compact(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}k", n as f64 / 1_000.0),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0),
    }
}

fn ago(unix_secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let secs = now.saturating_sub(unix_secs);
    match secs {
        0..=59 => format!("{secs}s ago"),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86_399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

#[derive(Default, PartialEq, Clone, Copy)]
enum Mode {
    #[default]
    Idle,
    Thinking,
    Text,
}

#[derive(Default)]
struct Renderer {
    mode: Mode,
}

impl Renderer {
    fn handle(&mut self, ev: StreamEvent<'_>) {
        match ev {
            StreamEvent::Thinking(t) => {
                if self.mode != Mode::Thinking {
                    print!("\n\x1b[2m· thinking\x1b[0m\n\x1b[2m");
                    self.mode = Mode::Thinking;
                }
                print!("{t}");
            }
            StreamEvent::Text(t) => {
                if self.mode != Mode::Text {
                    if self.mode == Mode::Thinking {
                        print!("\x1b[0m");
                    }
                    println!();
                    self.mode = Mode::Text;
                }
                print!("{t}");
            }
            StreamEvent::ToolCallStart { name } => {
                if self.mode == Mode::Thinking {
                    print!("\x1b[0m");
                }
                print!("\n\x1b[36m→ {name}\x1b[0m\n");
                self.mode = Mode::Idle;
            }
        }
        let _ = std::io::stdout().flush();
    }
}
