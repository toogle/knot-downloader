use std::{collections::HashMap, env, fs, path::Path, process::ExitCode, time::Duration};

use anyhow::{Context, Result};
use reqwest::{Client, StatusCode, header};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(with = "humantime_serde")]
    interval: Duration,
    create_directories: bool,
    files: Vec<FileEntry>,
}

#[derive(Debug, Deserialize)]
struct FileEntry {
    url: String,
    path: String,
}

async fn run() -> Result<()> {
    let config_path = env::var("CONFIG_PATH").unwrap_or("config.yml".to_string());
    let config = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file from {config_path:?}"))?;
    let config: Config = serde_yaml::from_str(&config)
        .with_context(|| format!("Failed to parse config file from {config_path:?}"))?;

    let client = Client::new();
    let mut etags = HashMap::new();

    loop {
        for FileEntry { url, path } in &config.files {
            let mut req = client.get(url);
            if let Some(etag) = etags.get(url) {
                req = req.header(header::IF_NONE_MATCH, etag);
            }

            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Some(etag) = resp.headers().get(header::ETAG) {
                        etags.insert(url, etag.to_str().unwrap().to_string());
                    }

                    if config.create_directories {
                        let dir = Path::new(&path).parent().unwrap();
                        fs::create_dir_all(dir).with_context(|| {
                            format!("Failed to create directories for {path:?}")
                        })?;
                    }

                    let body = resp
                        .text()
                        .await
                        .with_context(|| format!("Failed to read response body from {url:?}"))?;
                    let body_len = human_bytes::human_bytes(body.len() as f64);
                    fs::write(path, body)
                        .with_context(|| format!("Failed to write file to {path:?}"))?;

                    println!("Downloaded {} to {} ({})", url, path, body_len);
                }
                Ok(resp) if resp.status() == StatusCode::NOT_MODIFIED => {
                    println!("Skipped {} (not modified)", url)
                }
                Ok(resp) => eprintln!("Failed to download {}: {}", url, resp.status()),
                Err(err) => eprintln!("Failed to download {}: {}", url, err),
            }
        }

        tokio::time::sleep(config.interval).await;
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(err) = run().await {
        eprintln!("Error: {err}\n\nCaused by:");
        for cause in err.chain().skip(1) {
            eprintln!("  {cause}");
        }
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
