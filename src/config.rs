use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::events::MessageFormat;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub discord: DiscordConfig,
    #[serde(default)]
    pub defaults: DefaultsConfig,
    #[serde(default)]
    pub routes: Vec<RouteRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordConfig {
    pub bot_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    pub channel: Option<String>,
    #[serde(default)]
    pub format: MessageFormat,
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            channel: None,
            format: MessageFormat::Compact,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouteRule {
    pub event: String,
    #[serde(default)]
    pub filter: BTreeMap<String, String>,
    pub channel: Option<String>,
    pub format: Option<MessageFormat>,
    pub template: Option<String>,
}

pub fn default_config_path() -> PathBuf {
    if let Ok(override_path) = env::var("CLAWHIP_CONFIG") {
        return PathBuf::from(override_path);
    }

    let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".clawhip").join("config.toml")
}

impl AppConfig {
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(path)?;
        Ok(toml::from_str(&raw)?)
    }

    pub fn to_pretty_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.to_pretty_toml()?)?;
        Ok(())
    }

    pub fn effective_token(&self) -> Option<String> {
        env::var("CLAWHIP_DISCORD_BOT_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| self.discord.bot_token.clone())
    }

    pub fn run_interactive_editor(&mut self, path: &Path) -> Result<()> {
        println!("clawhip config editor");
        println!("Path: {}", path.display());
        println!();

        loop {
            self.print_summary();
            println!("Choose an action:");
            println!("  1) Set Discord bot token");
            println!("  2) Set default channel");
            println!("  3) Set default format");
            println!("  4) Add route");
            println!("  5) Remove route");
            println!("  6) Save and exit");
            println!("  7) Exit without saving");

            match prompt("Selection")?.trim() {
                "1" => self.discord.bot_token = empty_to_none(prompt("Bot token")?),
                "2" => self.defaults.channel = empty_to_none(prompt("Default channel")?),
                "3" => self.defaults.format = prompt_format(Some(self.defaults.format.clone()))?,
                "4" => self.add_route()?,
                "5" => self.remove_route()?,
                "6" => {
                    self.save(path)?;
                    println!("Saved {}", path.display());
                    break;
                }
                "7" => {
                    println!("Discarded changes.");
                    break;
                }
                _ => println!("Unknown selection."),
            }
            println!();
        }

        Ok(())
    }

    fn print_summary(&self) {
        let token_status = if self
            .discord
            .bot_token
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        {
            "missing"
        } else {
            "configured"
        };
        println!("Current config summary:");
        println!("  Discord token: {token_status}");
        println!(
            "  Default channel: {}",
            self.defaults.channel.as_deref().unwrap_or("<unset>")
        );
        println!("  Default format: {}", self.defaults.format.as_str());
        if self.routes.is_empty() {
            println!("  Routes: <none>");
        } else {
            println!("  Routes:");
            for (index, route) in self.routes.iter().enumerate() {
                let filter = if route.filter.is_empty() {
                    "<none>".to_string()
                } else {
                    route
                        .filter
                        .iter()
                        .map(|(key, value)| format!("{key}={value}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                println!(
                    "    [{}] event={}, filter={}, channel={}, format={}, template={}",
                    index,
                    route.event,
                    filter,
                    route.channel.as_deref().unwrap_or("<default>"),
                    route
                        .format
                        .as_ref()
                        .map(MessageFormat::as_str)
                        .unwrap_or("<default>"),
                    route.template.as_deref().unwrap_or("<default>")
                );
            }
        }
        println!();
    }

    fn add_route(&mut self) -> Result<()> {
        let event = prompt("Event pattern (examples: github.*, tmux.*, git.commit)")?;
        let event = event.trim().to_string();
        if event.is_empty() {
            println!("Event pattern cannot be empty.");
            return Ok(());
        }
        let filter = prompt("Filter pairs (comma-separated key=value, optional)")?;
        let channel = prompt("Route channel (blank = use default)")?;
        let format = prompt_format(None)?;
        let template = prompt("Route template (blank = use built-in formatter)")?;
        self.routes.push(RouteRule {
            event,
            filter: parse_filter_map(&filter),
            channel: empty_to_none(channel),
            format: Some(format),
            template: empty_to_none(template),
        });
        Ok(())
    }

    fn remove_route(&mut self) -> Result<()> {
        let index = prompt("Route index to remove")?;
        let index: usize = index.trim().parse()?;
        if index < self.routes.len() {
            self.routes.remove(index);
            println!("Removed route {index}");
        } else {
            println!("No route at index {index}");
        }
        Ok(())
    }
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    Ok(value.trim_end().to_string())
}

fn prompt_format(default: Option<MessageFormat>) -> Result<MessageFormat> {
    let default_value = default.unwrap_or(MessageFormat::Compact);
    let input = prompt(&format!(
        "Format [{}] (compact/alert/inline/raw)",
        default_value.as_str()
    ))?;
    if input.trim().is_empty() {
        return Ok(default_value);
    }
    MessageFormat::from_label(input.trim())
}

fn parse_filter_map(input: &str) -> BTreeMap<String, String> {
    input
        .split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .filter(|(key, value)| !key.is_empty() && !value.is_empty())
        .collect()
}

fn empty_to_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
