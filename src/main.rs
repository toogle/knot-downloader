use std::{collections::HashMap, env, fs, io, path::Path, process::ExitCode, time::Duration};

use anyhow::{Context, Result};
use chrono::Local;
use colored::Colorize;
use fern::{
    Dispatch,
    colors::{Color, ColoredLevelConfig},
};
use imara_diff::{Algorithm, Diff, InternedInput};
use log::LevelFilter;
use reqwest::{Client, StatusCode, header};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(with = "humantime_serde")]
    interval: Duration,
    #[serde(default)]
    create_directories: bool,
    log_level: LevelFilter,
    files: Vec<FileEntry>,
}

#[derive(Debug, Deserialize)]
struct FileEntry {
    url: String,
    path: String,
}

fn setup_logger(level: LevelFilter) -> Result<()> {
    let colors = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::Green)
        .debug(Color::BrightBlack);

    Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{} {:<5} {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                colors.color(record.level()),
                message,
            ))
        })
        .filter(|metadata| metadata.target() == module_path!())
        .level(level)
        .chain(io::stdout())
        .apply()
        .context("Failed to setup logger")?;

    Ok(())
}

async fn wait_for_shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut sigterm =
            signal(SignalKind::terminate()).context("Failed to install SIGTERM handler")?;
        let mut sigint =
            signal(SignalKind::interrupt()).context("Failed to install SIGINT handler")?;

        tokio::select! {
            _ = sigterm.recv() => {},
            _ = sigint.recv() => {},
        }
    }

    #[cfg(windows)]
    {
        use tokio::signal::windows;

        let mut ctrl_c = windows::ctrl_c().context("Failed to install Ctrl-C handler")?;
        let mut ctrl_break =
            windows::ctrl_break().context("Failed to install Ctrl-Break handler")?;
        let mut ctrl_close =
            windows::ctrl_close().context("Failed to install Ctrl-Close handler")?;
        let mut ctrl_shutdown =
            windows::ctrl_shutdown().context("Failed to install Ctrl-Shutdown handler")?;

        tokio::select! {
            _ = ctrl_c.recv() => {},
            _ = ctrl_break.recv() => {},
            _ = ctrl_close.recv() => {},
            _ = ctrl_shutdown.recv() => {},
        }
    }

    Ok(())
}

async fn download_files(files: &[FileEntry], interval: Duration) -> Result<()> {
    let client = Client::new();
    let mut etags = HashMap::new();

    loop {
        for FileEntry { url, path } in files {
            let mut req = client.get(url);
            if let Some(etag) = etags.get(url) {
                req = req.header(header::IF_NONE_MATCH, etag);
            }

            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Some(etag) = resp.headers().get(header::ETAG) {
                        etags.insert(url, etag.to_str().unwrap().to_string());
                    }

                    let body = resp
                        .text()
                        .await
                        .with_context(|| format!("Failed to read response body from {url:?}"))?;
                    let body_len = human_bytes::human_bytes(body.len() as f64);

                    let current = fs::read_to_string(path).unwrap_or_default();
                    let input = InternedInput::new(current.as_str(), body.as_str());
                    let diff = Diff::compute(Algorithm::Histogram, &input);

                    if diff.count_additions() > 0 || diff.count_removals() > 0 {
                        fs::write(path, body)
                            .with_context(|| format!("Failed to write file to {path:?}"))?;

                        log::info!(
                            "Downloaded {} to {} ({}, {}/{})",
                            url,
                            path,
                            body_len,
                            format!("+{}", diff.count_additions()).green(),
                            format!("-{}", diff.count_removals()).red(),
                        );
                    } else {
                        log::debug!("Skipped {} (no changes)", url);
                    }
                }
                Ok(resp) if resp.status() == StatusCode::NOT_MODIFIED => {
                    log::debug!("Skipped {} (not modified)", url)
                }
                Ok(resp) => log::error!("Failed to download {}: {}", url, resp.status()),
                Err(err) => log::error!("Failed to download {}: {}", url, err),
            }
        }

        tokio::time::sleep(interval).await;
    }
}

async fn run() -> Result<()> {
    let config_path = env::var("CONFIG_PATH").unwrap_or("config.yml".to_string());
    let config = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file from {config_path:?}"))?;
    let config: Config = serde_yaml::from_str(&config)
        .with_context(|| format!("Failed to parse config file from {config_path:?}"))?;

    setup_logger(config.log_level)?;

    if config.create_directories {
        for FileEntry { url: _, path } in &config.files {
            if let Some(parent) = Path::new(&path).parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directories for {path:?}"))?;
            }
        }
    }

    loop {
        tokio::select! {
            res = download_files(&config.files, config.interval) => { res? }
            res = wait_for_shutdown_signal() => {
                res?;
                log::warn!("Shutting down...");
                break;
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    // Force colored output in Docker environments
    colored::control::set_override(true);

    if let Err(err) = run().await {
        eprintln!("Error: {err}\n\nCaused by:");
        for cause in err.chain().skip(1) {
            eprintln!("  {cause}");
        }
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
