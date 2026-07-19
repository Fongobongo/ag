//! Chat gateway: bridge a chat platform (Telegram first) to the control-plane
//! HTTP API so an operator can drive the grid from a phone.
//!
//! One provider trait, one Telegram implementation (raw reqwest to the Telegram
//! Bot API; no chat-client crate). Discord / WhatsApp are stubbed behind the
//! same trait — see the "not implemented" arms. WhatsApp in particular has no
//! easy open bot API (Business API is heavy and gated), so it is honestly
//! deferred rather than faked.
//!
//! Auth: `/start` and `/whoami` are open to anyone (they just echo your chat
//! id and tell you the host-side command to confirm ownership). Every other
//! command requires your chat id to be in the allowlist — a per-line file
//! (`AGENTGRID_GATEWAY_ADMINS_FILE`, default `~/.config/agentgrid/gateway-admins.txt`)
//! or comma-list `AGENTGRID_GATEWAY_ADMINS`. An operator with shell access to
//! the host running the gateway runs `agentgrid-gateway allow <chat_id>` to add
//! a chat. The file is re-read on every message, so approval takes effect
//! immediately without restarting the bot.
//!
//! Commands: /help /nodes /tasks /run <repo> <adapter> <prompt...>
//!           /show <id> /cancel <id> /logs <id> /whoami

use std::time::Duration;

use agentgrid_common::CreateTaskRequest;
use anyhow::Result;

/// Where the persisted allowlist of admin chat ids lives. `allow` writes here,
/// `run` reads it on every message (cheap — tiny file). Override with env.
fn admins_file() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("AGENTGRID_GATEWAY_ADMINS_FILE") {
        return std::path::PathBuf::from(p);
    }
    let mut p = config_dir();
    p.push("agentgrid");
    let _ = std::fs::create_dir_all(&p);
    p.push("gateway-admins.txt");
    p
}

fn config_dir() -> std::path::PathBuf {
    if let Some(d) = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
    {
        return d;
    }
    if let Some(h) = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")) {
        return h;
    }
    std::path::PathBuf::from(".")
}

/// Load the allowlist: the persisted file (one id per line) plus any
/// comma-separated ids in `AGENTGRID_GATEWAY_ADMINS` (bootstrap/override).
fn load_admins() -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    if let Ok(s) = std::env::var("AGENTGRID_GATEWAY_ADMINS") {
        out.extend(s.split(',').filter_map(|x| x.trim().parse::<i64>().ok()));
    }
    if let Ok(s) = std::fs::read_to_string(admins_file()) {
        out.extend(s.lines().filter_map(|x| x.trim().parse::<i64>().ok()));
    }
    out
}

#[derive(clap::Parser)]
#[command(name = "agentgrid-gateway")]
enum Args {
    /// Run the chat bridge (Telegram long-poll loop).
    Run(RunArgs),
    /// Approve a Telegram chat id on this host so it can drive the gateway.
    /// The chat learns its id from `/start` / `/whoami`, then an operator with
    /// shell access to this host runs this command to confirm ownership.
    Allow(AllowArgs),
}

#[derive(clap::Args)]
struct RunArgs {
    /// Control-plane base URL, e.g. http://127.0.0.1:7800
    #[arg(long, env = "AGENTGRID_SERVER")]
    control_plane: String,
    /// A JWT for a control-plane user (operator). Created with `ag login`.
    #[arg(long, env = "AGENTGRID_GATEWAY_TOKEN")]
    token: String,
    /// Telegram bot token from @BotFather. Omit to disable Telegram.
    #[arg(long, env = "AGENTGRID_GATEWAY_TELEGRAM_TOKEN")]
    telegram: Option<String>,
}

#[derive(clap::Args)]
struct AllowArgs {
    /// The numeric Telegram chat id to approve.
    chat_id: i64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "agentgrid_gateway=info".into()),
        )
        .init();
    match clap::Parser::parse() {
        Args::Run(a) => run(a).await,
        Args::Allow(a) => {
            let path = admins_file();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut s = std::fs::read_to_string(&path).unwrap_or_default();
            if !s.is_empty() && !s.ends_with('\n') {
                s.push('\n');
            }
            s.push_str(&format!("{}\n", a.chat_id));
            std::fs::write(&path, s)?;
            println!(
                "approved chat {} (allowlist: {})",
                a.chat_id,
                path.display()
            );
            Ok(())
        }
    }
}

async fn run(args: RunArgs) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    let ctl = ControlPlane::new(&client, &args.control_plane, &args.token);

    let provider: Box<dyn ChatProvider> = if let Some(tok) = args.telegram.as_deref() {
        Box::new(Telegram::new(tok.to_string()))
    } else {
        tracing::warn!("no chat provider configured (only --telegram supported); nothing to do");
        return Ok(());
    };
    tracing::info!(
        "gateway up: provider=telegram, control_plane={} (allowlist re-read per message)",
        args.control_plane
    );
    provider.run(&client, &ctl).await
}

/// A control-plane HTTP client for the handful of endpoints the gateway uses.
struct ControlPlane<'a> {
    client: &'a reqwest::Client,
    base: &'a str,
    token: &'a str,
}

impl<'a> ControlPlane<'a> {
    fn new(client: &'a reqwest::Client, base: &'a str, token: &'a str) -> Self {
        Self {
            client,
            base,
            token,
        }
    }
    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .get(format!("{}{}", self.base, path))
            .bearer_auth(self.token)
    }
    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .post(format!("{}{}", self.base, path))
            .bearer_auth(self.token)
    }

    async fn nodes(&self) -> Result<String> {
        let v: serde_json::Value = self.get("/v1/nodes").send().await?.json().await?;
        Ok(fmt_nodes(&v))
    }
    async fn tasks(&self) -> Result<String> {
        let v: serde_json::Value = self.get("/v1/tasks").send().await?.json().await?;
        Ok(fmt_tasks(&v))
    }
    async fn show(&self, id: &str) -> Result<String> {
        let r = self.get(&format!("/v1/tasks/{id}")).send().await?;
        if !r.status().is_success() {
            return Ok(format!("task {id} not found ({})", r.status()));
        }
        let v: serde_json::Value = r.json().await?;
        let st = v.get("status").and_then(|x| x.as_str()).unwrap_or("?");
        let p = v.get("prompt").and_then(|x| x.as_str()).unwrap_or("");
        let repo = v.get("repository").and_then(|x| x.as_str()).unwrap_or("?");
        let adapter = v.get("adapter").and_then(|x| x.as_str()).unwrap_or("?");
        Ok(format!(
            "task {id}\nstatus: {st}\nrepo: {repo}\nadapter: {adapter}\nprompt: {p}"
        ))
    }
    async fn run(&self, repo: &str, adapter: &str, prompt: &str) -> Result<String> {
        let req = CreateTaskRequest {
            prompt: prompt.to_string(),
            repository: repo.to_string(),
            adapter: adapter.to_string(),
            requested_node_id: None,
            timeout_secs: None,
            validation_command: None,
            base_commit: None,
        };
        let r = self.post("/v1/tasks").json(&req).send().await?;
        let status = r.status();
        let body = r.text().await.unwrap_or_default();
        if !status.is_success() {
            return Ok(format!("create task failed ({status}): {body}"));
        }
        let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("?");
        let st = v.get("status").and_then(|x| x.as_str()).unwrap_or("?");
        Ok(format!("task {id} created ({st})"))
    }
    async fn cancel(&self, id: &str) -> Result<String> {
        let r = self.post(&format!("/v1/tasks/{id}/cancel")).send().await?;
        Ok(format!("cancel {id}: {}", r.status()))
    }
    async fn logs(&self, id: &str) -> Result<String> {
        let r = self.get(&format!("/v1/tasks/{id}/events")).send().await?;
        let v: serde_json::Value = r.json().await.unwrap_or(serde_json::Value::Array(vec![]));
        let arr = v.as_array().cloned().unwrap_or_default();
        if arr.is_empty() {
            return Ok(format!("no events for {id}"));
        }
        let mut out = String::new();
        for (i, e) in arr.iter().take(20).enumerate() {
            let kind = e.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let data = e.get("data").map(|v| v.to_string()).unwrap_or_default();
            out.push_str(&format!("{} {kind}: {data}\n", i));
        }
        if arr.len() > 20 {
            out.push_str(&format!("... ({} more)\n", arr.len() - 20));
        }
        Ok(out)
    }
}

/// A chat platform the gateway can speak to: receive messages and reply.
trait ChatProvider: Send {
    /// Run the receive/dispatch loop until the process is asked to stop.
    fn run<'a>(
        self: Box<Self>,
        client: &'a reqwest::Client,
        ctl: &'a ControlPlane<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;
}

fn allowed(chat_id: i64) -> bool {
    load_admins().contains(&chat_id)
}

async fn dispatch(ctl: &ControlPlane<'_>, text: &str) -> String {
    let mut parts = text.split_whitespace();
    let cmd = parts.next().unwrap_or("");
    // strip an optional leading bot mention like "/nodes@botname"
    let cmd = cmd.split('@').next().unwrap_or(cmd).trim_start_matches('/');
    match cmd {
        "help" | "start" => HELP.to_string(),
        "nodes" => ctl.nodes().await.unwrap_or_else(|e| e.to_string()),
        "tasks" => ctl.tasks().await.unwrap_or_else(|e| e.to_string()),
        "show" => match parts.next() {
            Some(id) => ctl.show(id).await.unwrap_or_else(|e| e.to_string()),
            None => "usage: /show <task-id>".into(),
        },
        "cancel" => match parts.next() {
            Some(id) => ctl.cancel(id).await.unwrap_or_else(|e| e.to_string()),
            None => "usage: /cancel <task-id>".into(),
        },
        "logs" => match parts.next() {
            Some(id) => ctl.logs(id).await.unwrap_or_else(|e| e.to_string()),
            None => "usage: /logs <task-id>".into(),
        },
        "run" => {
            let repo = parts.next();
            let adapter = parts.next();
            let prompt: String = parts.collect::<Vec<_>>().join(" ");
            match (repo, adapter) {
                (Some(repo), Some(adapter)) if !prompt.is_empty() => ctl
                    .run(repo, adapter, &prompt)
                    .await
                    .unwrap_or_else(|e| e.to_string()),
                _ => "usage: /run <repo-url> <adapter> <prompt...>".into(),
            }
        }
        _ => format!("unknown command: {cmd} — try /help"),
    }
}

const HELP: &str = "agentgrid gateway — /help /whoami /nodes /tasks /show <id> /cancel <id> /logs <id> /run <repo-url> <adapter> <prompt...>. /start and /whoami are open (they show your chat id + the host-side approval command).";

// ---- formatting ----

fn fmt_nodes(v: &serde_json::Value) -> String {
    let arr = match v.as_array() {
        Some(a) if !a.is_empty() => a,
        _ => return "(no nodes)".into(),
    };
    let mut s = format!(
        "{:<12} {:<10} {:<3}/{:<3} {:<10}\n",
        "NODE", "STATUS", "ACT", "MAX", "DISK"
    );
    for n in arr {
        let name = n.get("name").and_then(|v| v.as_str()).unwrap_or("-");
        let st = n.get("status").and_then(|v| v.as_str()).unwrap_or("-");
        let act = n
            .get("active_attempts")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let max = n
            .get("max_concurrency")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let disk = n.get("free_disk_mb").and_then(|v| v.as_u64()).unwrap_or(0);
        let disk = if disk < 1024 {
            format!("{disk} MB !")
        } else {
            format!("{:.0} GB", disk as f64 / 1024.0)
        };
        s.push_str(&format!(
            "{name:<12} {st:<10} {act:<3}/{max:<3} {disk:<10}\n"
        ));
    }
    s
}

fn fmt_tasks(v: &serde_json::Value) -> String {
    let arr = match v.as_array() {
        Some(a) if !a.is_empty() => a,
        _ => return "(no tasks)".into(),
    };
    let mut s = format!("{:<12} {:<36} {:<12}\n", "REPO", "ID", "STATUS");
    for t in arr {
        let id = t.get("id").and_then(|v| v.as_str()).unwrap_or("-");
        let st = t.get("status").and_then(|v| v.as_str()).unwrap_or("-");
        let repo = t.get("repository").and_then(|v| v.as_str()).unwrap_or("-");
        s.push_str(&format!("{repo:<12} {id:<36} {st:<12}\n"));
    }
    s
}

// ---- Telegram provider (raw Bot API over reqwest, no chat crate) ----

struct Telegram {
    token: String,
    offset: std::sync::atomic::AtomicI64,
}

impl Telegram {
    fn new(token: String) -> Self {
        Self {
            token,
            offset: std::sync::atomic::AtomicI64::new(0),
        }
    }
    fn url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.token, method)
    }
}

impl ChatProvider for Telegram {
    fn run<'a>(
        self: Box<Self>,
        client: &'a reqwest::Client,
        ctl: &'a ControlPlane<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        let tg = self;
        Box::pin(async move {
            loop {
                let offset = tg.offset.load(std::sync::atomic::Ordering::Relaxed);
                let resp: serde_json::Value = match client
                    .post(tg.url("getUpdates"))
                    .json(&serde_json::json!({"offset": offset, "timeout": 30}))
                    .send()
                    .await
                {
                    Ok(r) => match r.json().await {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!("getUpdates parse: {e}");
                            tokio::time::sleep(Duration::from_secs(3)).await;
                            continue;
                        }
                    },
                    Err(e) => {
                        tracing::warn!("getUpdates: {e}");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };
                let updates = resp
                    .get("result")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                for u in updates {
                    let id = u.get("update_id").and_then(|v| v.as_i64()).unwrap_or(0);
                    tg.offset
                        .store(id + 1, std::sync::atomic::Ordering::Relaxed);
                    let msg = match u.get("message").or_else(|| u.get("edited_message")) {
                        Some(m) => m,
                        None => continue,
                    };
                    let chat_id = msg
                        .get("chat")
                        .and_then(|c| c.get("id"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let text = msg
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !text.starts_with('/') {
                        continue;
                    }
                    // /start and /whoami are open: echo the chat id + the
                    // host-side approval command so an operator can confirm
                    // ownership. Everything else needs the chat in the allowlist.
                    let cmd_only = text
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .split('@')
                        .next()
                        .unwrap_or("")
                        .trim_start_matches('/');
                    if matches!(cmd_only, "start" | "whoami") {
                        tracing::info!("tg {chat_id}: open command {cmd_only}");
                        let reply = format!(
                            "your chat id is {chat_id}\n\
to drive the gateway, run on this host:\n\
  agentgrid-gateway allow {chat_id}\n\
then send /nodes\n\
(this confirms you have shell access to the host where the gateway runs)"
                        );
                        let _ = client
                            .post(tg.url("sendMessage"))
                            .json(&serde_json::json!({"chat_id": chat_id, "text": reply}))
                            .send()
                            .await;
                        continue;
                    }
                    if !allowed(chat_id) {
                        tracing::info!("ignoring chat {chat_id} (not in allowlist)");
                        continue;
                    }
                    tracing::info!("tg {chat_id}: {text}");
                    let reply = dispatch(ctl, &text).await;
                    let _ = client
                        .post(tg.url("sendMessage"))
                        .json(&serde_json::json!({"chat_id": chat_id, "text": reply}))
                        .send()
                        .await;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_nodes_marks_low_disk() {
        let v: serde_json::Value = serde_json::json!([
            {"name":"a","status":"online","active_attempts":0,"max_concurrency":2,"free_disk_mb":500},
            {"name":"b","status":"degraded","active_attempts":1,"max_concurrency":4,"free_disk_mb":4096}
        ]);
        let s = fmt_nodes(&v);
        assert!(s.contains("500 MB !"));
        assert!(s.contains("4 GB"));
        assert!(s.contains("degraded"));
    }

    #[test]
    fn fmt_nodes_empty() {
        assert_eq!(fmt_nodes(&serde_json::Value::Array(vec![])), "(no nodes)");
    }

    #[test]
    fn fmt_tasks_lists_rows() {
        let v: serde_json::Value = serde_json::json!([
            {"id":"abc","status":"running","repository":"r1"}
        ]);
        let s = fmt_tasks(&v);
        assert!(s.contains("abc"));
        assert!(s.contains("running"));
        assert!(s.contains("r1"));
    }
}
