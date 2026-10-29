use clap::{Parser, Subcommand};
use orbital::config::Config;
use orbital::{bot, db, seed};

#[derive(Parser)]
#[command(name = "orbital", about = "Orbital — themed verticals of people and their links")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve the site.
    Serve,
    /// Apply migrations and exit.
    Migrate,
    /// Create the starter verticals and the owner accounts (idempotent).
    Seed,
    /// Bluesky bot and housekeeping.
    Bot {
        #[command(subcommand)]
        what: BotCmd,
    },
}

#[derive(Subcommand)]
enum BotCmd {
    /// Post up to three member spotlights.
    Spotlight {
        #[arg(long)]
        dry_run: bool,
    },
    /// Post the weekly top-5 roundup for one vertical (or today's rotation).
    Roundup {
        #[arg(long)]
        vertical: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Promote trust levels and prune old click events.
    Maintain,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "orbital=info,tower_http=warn".into()),
        )
        .init();

    let cfg = Config::from_env()?;
    match Cli::parse().cmd {
        Cmd::Migrate => {
            let pool = db::connect(&cfg).await?;
            db::migrate(&pool).await?;
            println!(
                "migrated; schema version {:?}",
                db::applied_version(&pool).await?
            );
        }
        Cmd::Seed => {
            let pool = db::connect(&cfg).await?;
            db::migrate(&pool).await?;
            let report = seed::seed(&pool).await?;
            if report.credentials.is_empty() {
                println!("seed: accounts already existed, nothing minted");
            } else {
                for (handle, password, role) in &report.credentials {
                    println!("seed: {handle} ({role}) password={password}");
                }
            }
        }
        Cmd::Bot { what } => {
            let state = orbital::build_state(cfg).await?;
            match what {
                BotCmd::Spotlight { dry_run } => {
                    bot::spotlight(&state.pool, &state.cfg, &state.http, dry_run).await?
                }
                BotCmd::Roundup { vertical, dry_run } => {
                    bot::roundup(
                        &state.pool,
                        &state.cfg,
                        &state.http,
                        vertical.as_deref(),
                        dry_run,
                    )
                    .await?
                }
                BotCmd::Maintain => bot::maintenance(&state.pool).await?,
            }
        }
        Cmd::Serve => serve(cfg).await?,
    }
    Ok(())
}

async fn serve(cfg: Config) -> anyhow::Result<()> {
    let addr = cfg.bind_addr.clone();
    let state = orbital::build_state(cfg).await?;
    db::migrate(&state.pool).await?;
    let app = orbital::build_router(state).await;
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("orbital listening on {addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
