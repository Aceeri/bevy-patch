use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;

#[derive(Parser)]
#[command(name = "bevy-patch")]
#[command(about = "Generate bevy patch entries")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Path {
        path: String,
    },
    Git {
        #[arg(long, default_value = "https://github.com/bevyengine/bevy")]
        repo: String,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        rev: Option<String>,
    },
    // Github { // todo: add shorthand for pull request fetching
    //     #[arg(long, default_value = "https://github.com/bevyengine/bevy")]
    //     repo: String,
    //     #[arg(long)]
    //     pr: String, // #123456
    // },
}

#[derive(Deserialize)]
struct GithubContent {
    name: String,
    #[serde(rename = "type")]
    content_type: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubError {
    message: String,
    // documentation_url: Option<String>,
    status: String,
}

impl std::error::Error for GithubError {}

impl std::fmt::Display for GithubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.status, self.message)
    }
}

fn fetch_crates_from_local(path: &str) -> Result<Vec<String>> {
    let dir = std::fs::read_dir(path.to_owned() + "/crates")?;
    let mut crates = Vec::new();
    for c in dir {
        let c = c?;
        let name = c.file_name();
        let ty = c.file_type()?;
        if !ty.is_dir() {
            continue;
        }

        crates.push(
            name.into_string()
                .map_err(|_| anyhow::anyhow!("couldn't convert os string"))?,
        );
    }

    Ok(crates)
}

fn fetch_crates_from_github(repo: &str, git_ref: &str) -> Result<Vec<String>> {
    if !repo.contains("github.com") {
        // todo: error out here
        return Err(anyhow::anyhow!(
            "Repo is currently expected to be from github.com"
        ));
    }

    let mut api_url = if repo.starts_with("github.com") {
        "https://".to_owned() + repo
    } else {
        repo.to_owned()
    };

    if api_url.ends_with(".git") {
        api_url = api_url[0..api_url.len() - 4].to_owned();
    }

    api_url = api_url
        .replace("http://", "https://")
        .replace("github.com/", "api.github.com/repos/");

    let url = format!("{}/contents/crates?ref={}", api_url, git_ref);

    let client = reqwest::blocking::Client::new();
    let response = client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .header("User-Agent", "bevy-patch")
        .send()
        .context("Failed to fetch from GitHub")?;

    if response.status() == 200 {
        let content: Vec<GithubContent> =
            response.json().context("Failed to parse GitHub response")?;

        let mut crates: Vec<String> = content
            .into_iter()
            .filter(|c| c.content_type == "dir")
            .map(|c| c.name)
            .collect();

        crates.sort();
        Ok(crates)
    } else {
        let err: GithubError = response.json().context("Failed to parse GitHub response")?;
        Err(anyhow::anyhow!(err))
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut result = Vec::new();
    result.push("[patch.crates-io]".to_owned());
    result.push("# Bevy Patch".to_owned());

    match cli.command {
        Command::Path { path } => {
            let crates = fetch_crates_from_local(&path)?;

            result.push(format!("bevy = {{ path = \"{path}\" }}"));

            for c in crates {
                result.push(format!("{c} = {{ path = \"{path}/crates/{c}\" }}"));
            }
        }
        Command::Git {
            repo,
            branch,
            tag,
            rev,
        } => {
            let git_ref = tag
                .as_deref()
                .or(branch.as_deref())
                .or(rev.as_deref())
                .unwrap_or("main");

            let crates = fetch_crates_from_github(&repo, git_ref)?;

            let specifier = if let Some(tag) = &tag {
                format!("tag = \"{tag}\"")
            } else if let Some(branch) = &branch {
                format!("branch = \"{branch}\"")
            } else if let Some(rev) = &rev {
                format!("rev = \"{rev}\"")
            } else {
                "branch = \"main\"".to_string()
            };

            result.push(format!("bevy = {{ git = \"{repo}\", {specifier} }}"));
            for c in crates {
                result.push(format!("{c} = {{ git = \"{repo}\", {specifier} }}"));
            }
        }
    }

    println!("{}", result.join("\n"));
    Ok(())
}
