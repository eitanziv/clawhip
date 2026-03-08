use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

use tokio::process::Command;
use tokio::time::sleep;

use crate::Result;
use crate::cli::TmuxNewArgs;
use crate::discord::DiscordClient;
use crate::events::IncomingEvent;
use crate::router::Router;

pub async fn run(
    args: TmuxNewArgs,
    router: std::sync::Arc<Router>,
    discord: std::sync::Arc<DiscordClient>,
) -> Result<()> {
    launch_session(&args).await?;

    let monitor_args = args.clone();
    let monitor_router = router.clone();
    let monitor_discord = discord.clone();
    let monitor = tokio::spawn(async move {
        monitor_session(monitor_args, monitor_router, monitor_discord).await
    });

    if args.attach {
        attach_session(&args.session).await?;
    }

    monitor.await??;
    Ok(())
}

#[derive(Clone)]
struct PaneState {
    session: String,
    pane_name: String,
    content_hash: u64,
    snapshot: String,
    last_change: Instant,
    last_stale_notification: Option<Instant>,
}

#[derive(Clone)]
struct PaneSnapshot {
    pane_id: String,
    session: String,
    pane_name: String,
    content: String,
}

#[derive(Clone)]
struct KeywordHit {
    keyword: String,
    line: String,
}

async fn launch_session(args: &TmuxNewArgs) -> Result<()> {
    let mut command = Command::new(tmux_bin());
    command
        .arg("new-session")
        .arg("-d")
        .arg("-s")
        .arg(&args.session);
    if let Some(window_name) = &args.window_name {
        command.arg("-n").arg(window_name);
    }
    if let Some(cwd) = &args.cwd {
        command.arg("-c").arg(cwd);
    }
    if !args.command.is_empty() {
        command.arg("--");
        command.args(&args.command);
    }

    let output = command.output().await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_string()
            .into())
    }
}

async fn attach_session(session: &str) -> Result<()> {
    let output = Command::new(tmux_bin())
        .arg("attach-session")
        .arg("-t")
        .arg(session)
        .output()
        .await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_string()
            .into())
    }
}

async fn monitor_session(
    args: TmuxNewArgs,
    router: std::sync::Arc<Router>,
    discord: std::sync::Arc<DiscordClient>,
) -> Result<()> {
    let mut state: HashMap<String, PaneState> = HashMap::new();
    let poll_interval = Duration::from_secs(
        std::env::var("CLAWHIP_TMUX_POLL_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(5),
    );
    let stale_after = Duration::from_secs(args.stale_minutes.max(1) * 60);
    let keywords = args
        .keywords
        .iter()
        .map(|keyword| keyword.trim().to_string())
        .filter(|keyword| !keyword.is_empty())
        .collect::<Vec<_>>();

    loop {
        if !session_exists(&args.session).await? {
            break;
        }

        let panes = snapshot_session(&args.session).await?;
        let mut active = HashSet::new();
        let now = Instant::now();

        for pane in panes {
            active.insert(pane.pane_id.clone());
            let pane_key = pane.pane_id.clone();
            let hash = content_hash(&pane.content);
            let latest_line = last_nonempty_line(&pane.content);

            match state.get_mut(&pane_key) {
                None => {
                    state.insert(
                        pane_key,
                        PaneState {
                            session: pane.session,
                            pane_name: pane.pane_name,
                            content_hash: hash,
                            snapshot: pane.content,
                            last_change: now,
                            last_stale_notification: None,
                        },
                    );
                }
                Some(existing) => {
                    if existing.content_hash != hash {
                        let hits =
                            collect_keyword_hits(&existing.snapshot, &pane.content, &keywords);
                        for hit in hits {
                            let event = IncomingEvent::tmux_keyword(
                                pane.session.clone(),
                                hit.keyword,
                                hit.line,
                                args.channel.clone(),
                            )
                            .with_format(args.format.map(Into::into));
                            dispatch_wrapper_event(
                                &event,
                                args.mention.as_deref(),
                                &router,
                                &discord,
                            )
                            .await?;
                        }

                        existing.session = pane.session;
                        existing.pane_name = pane.pane_name;
                        existing.content_hash = hash;
                        existing.snapshot = pane.content;
                        existing.last_change = now;
                        existing.last_stale_notification = None;
                    } else if now.duration_since(existing.last_change) >= stale_after
                        && existing
                            .last_stale_notification
                            .map(|previous| now.duration_since(previous) >= stale_after)
                            .unwrap_or(true)
                    {
                        let event = IncomingEvent::tmux_stale(
                            existing.session.clone(),
                            existing.pane_name.clone(),
                            args.stale_minutes,
                            latest_line,
                            args.channel.clone(),
                        )
                        .with_format(args.format.map(Into::into));
                        dispatch_wrapper_event(&event, args.mention.as_deref(), &router, &discord)
                            .await?;
                        existing.last_stale_notification = Some(now);
                    }
                }
            }
        }

        state.retain(|pane_id, _| active.contains(pane_id));
        sleep(poll_interval).await;
    }

    Ok(())
}

async fn dispatch_wrapper_event(
    event: &IncomingEvent,
    mention: Option<&str>,
    router: &Router,
    discord: &DiscordClient,
) -> Result<()> {
    let (channel, _format, content) = router.preview(event)?;
    let final_content = match mention {
        Some(mention) if !mention.trim().is_empty() => format!("{} {}", mention.trim(), content),
        _ => content,
    };
    discord.send_message(&channel, &final_content).await
}

async fn session_exists(session: &str) -> Result<bool> {
    let output = Command::new(tmux_bin())
        .arg("has-session")
        .arg("-t")
        .arg(session)
        .output()
        .await?;
    Ok(output.status.success())
}

async fn snapshot_session(session: &str) -> Result<Vec<PaneSnapshot>> {
    let output = Command::new(tmux_bin())
        .arg("list-panes")
        .arg("-t")
        .arg(session)
        .arg("-F")
        .arg("#{pane_id}|#{session_name}|#{window_index}.#{pane_index}|#{pane_title}")
        .output()
        .await?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_string()
            .into());
    }

    let mut panes = Vec::new();
    for line in String::from_utf8(output.stdout)?.lines() {
        let mut parts = line.splitn(4, '|');
        let pane_id = parts.next().unwrap_or_default().to_string();
        if pane_id.is_empty() {
            continue;
        }
        let session_name = parts.next().unwrap_or_default().to_string();
        let pane_name = parts.next().unwrap_or_default().to_string();
        let capture = Command::new(tmux_bin())
            .arg("capture-pane")
            .arg("-p")
            .arg("-t")
            .arg(&pane_id)
            .arg("-S")
            .arg("-200")
            .output()
            .await?;
        if !capture.status.success() {
            return Err(String::from_utf8_lossy(&capture.stderr)
                .trim()
                .to_string()
                .into());
        }
        panes.push(PaneSnapshot {
            pane_id,
            session: session_name,
            pane_name,
            content: String::from_utf8(capture.stdout)?,
        });
    }
    Ok(panes)
}

fn collect_keyword_hits(previous: &str, current: &str, keywords: &[String]) -> Vec<KeywordHit> {
    if keywords.is_empty() {
        return Vec::new();
    }

    let previous_lines: HashSet<&str> = previous.lines().collect();
    current
        .lines()
        .filter(|line| !previous_lines.contains(*line))
        .flat_map(|line| {
            keywords.iter().filter_map(move |keyword| {
                if line
                    .to_ascii_lowercase()
                    .contains(&keyword.to_ascii_lowercase())
                {
                    Some(KeywordHit {
                        keyword: keyword.clone(),
                        line: line.to_string(),
                    })
                } else {
                    None
                }
            })
        })
        .collect()
}

fn content_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

fn last_nonempty_line(content: &str) -> String {
    content
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("<no output>")
        .trim()
        .to_string()
}

fn tmux_bin() -> String {
    std::env::var("CLAWHIP_TMUX_BIN").unwrap_or_else(|_| "tmux".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_hits_only_emit_for_new_lines() {
        let hits = collect_keyword_hits(
            "done\nall good",
            "done\nall good\nerror: failed\nPR created #7",
            &["error".into(), "PR created".into()],
        );
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].keyword, "error");
        assert_eq!(hits[1].keyword, "PR created");
    }
}
