use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::events::MessageFormat;

#[derive(Debug, Parser)]
#[command(
    name = "clawhip",
    version,
    about = "Standalone event-to-channel notification gateway for Discord"
)]
pub struct Cli {
    /// Override the config file path.
    #[arg(long, global = true, env = "CLAWHIP_CONFIG")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    pub fn config_path(&self) -> PathBuf {
        self.config
            .clone()
            .unwrap_or_else(crate::config::default_config_path)
    }
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Send a custom notification.
    Custom {
        #[arg(long)]
        channel: Option<String>,
        #[arg(long)]
        message: String,
    },
    /// Emit git-related notifications.
    Git {
        #[command(subcommand)]
        command: GitCommands,
    },
    /// Emit GitHub-related notifications.
    Github {
        #[command(subcommand)]
        command: GithubCommands,
    },
    /// Emit tmux-related notifications and wrappers.
    Tmux {
        #[command(subcommand)]
        command: TmuxCommands,
    },
    /// Read JSON event objects from stdin.
    Stdin,
    /// Run an HTTP webhook receiver.
    Serve {
        #[arg(long, default_value_t = 8765)]
        port: u16,
    },
    /// Manage configuration.
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
    },
}

#[derive(Debug, Subcommand)]
pub enum GitCommands {
    /// Emit a git commit event.
    Commit {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        branch: String,
        #[arg(long)]
        commit: String,
        #[arg(long)]
        summary: String,
        #[arg(long)]
        channel: Option<String>,
    },
    /// Emit a git branch-changed event.
    BranchChanged {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        old_branch: String,
        #[arg(long)]
        new_branch: String,
        #[arg(long)]
        channel: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum GithubCommands {
    /// Emit a GitHub issue-opened event.
    IssueOpened {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        number: u64,
        #[arg(long)]
        title: String,
        #[arg(long)]
        channel: Option<String>,
    },
    /// Emit a pull-request status-changed event.
    PrStatusChanged {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        number: u64,
        #[arg(long)]
        title: String,
        #[arg(long)]
        old_status: String,
        #[arg(long)]
        new_status: String,
        #[arg(long, default_value = "")]
        url: String,
        #[arg(long)]
        channel: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum TmuxCommands {
    /// Emit a tmux keyword event.
    Keyword {
        #[arg(long)]
        session: String,
        #[arg(long)]
        keyword: String,
        #[arg(long)]
        line: String,
        #[arg(long)]
        channel: Option<String>,
    },
    /// Emit a tmux stale event.
    Stale {
        #[arg(long)]
        session: String,
        #[arg(long)]
        pane: String,
        #[arg(long)]
        minutes: u64,
        #[arg(long)]
        last_line: String,
        #[arg(long)]
        channel: Option<String>,
    },
    /// Launch a tmux session through clawhip and monitor its pane output.
    New(TmuxNewArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TmuxWrapperFormat {
    Compact,
    Alert,
    Inline,
}

impl From<TmuxWrapperFormat> for MessageFormat {
    fn from(value: TmuxWrapperFormat) -> Self {
        match value {
            TmuxWrapperFormat::Compact => MessageFormat::Compact,
            TmuxWrapperFormat::Alert => MessageFormat::Alert,
            TmuxWrapperFormat::Inline => MessageFormat::Inline,
        }
    }
}

#[derive(Debug, Clone, Args)]
pub struct TmuxNewArgs {
    /// tmux session name.
    #[arg(short = 's', long = "session")]
    pub session: String,

    /// Optional tmux window name.
    #[arg(short = 'n', long = "window-name")]
    pub window_name: Option<String>,

    /// Optional starting directory for tmux.
    #[arg(short = 'c', long = "cwd")]
    pub cwd: Option<String>,

    /// Which Discord channel to notify.
    #[arg(long)]
    pub channel: Option<String>,

    /// Mention/tag prefix to prepend to wrapper-generated notifications.
    #[arg(long)]
    pub mention: Option<String>,

    /// Comma-separated keyword patterns to watch for in pane output.
    #[arg(long, value_delimiter = ',')]
    pub keywords: Vec<String>,

    /// Fire a stale event if the pane has no new output for this many minutes.
    #[arg(long, default_value_t = 10)]
    pub stale_minutes: u64,

    /// Output format for wrapper-generated notifications.
    #[arg(long)]
    pub format: Option<TmuxWrapperFormat>,

    /// Attach after launching. The wrapper continues monitoring until the session exits.
    #[arg(long, default_value_t = false)]
    pub attach: bool,

    /// Command and arguments to run inside the tmux session, after `--`.
    #[arg(last = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Default, Subcommand)]
pub enum ConfigCommand {
    /// Open the interactive config editor.
    #[default]
    Interactive,
    /// Print the active config as TOML.
    Show,
    /// Print the config file path.
    Path,
}
