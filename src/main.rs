use anyhow::Result;
use clap::Parser;

mod cli;
mod config;
mod agent;
mod llm;
mod mcp;
mod session;
mod tools;
mod tui;

use cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    // ログ初期化
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    // CLI引数パース
    let cli = Cli::parse();

    // 実行
    cli.execute().await?;

    Ok(())
}
