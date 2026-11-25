mod config;
mod site;

use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, bail};
use axum::{
    Router,
    extract::State,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use clap::{Parser, Subcommand};
use config::Config;
use glob::Pattern;
use site::Site;

#[derive(Parser)]
#[command(about = "Build and preview a multilingual Markdown site")]
struct Args {
    /// Read settings from this file instead of the default configuration file.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    source: Option<PathBuf>,
    #[arg(long, global = true)]
    default_language: Option<String>,
    #[arg(long, global = true)]
    template: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write the generated HTML to a new directory.
    Build {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Preview the site, reloading source files when requested.
    Preview {
        #[arg(long)]
        bind: Option<SocketAddr>,
    },
    /// Report broken internal Markdown links and missing translations.
    Check,
}

#[derive(Clone)]
struct Source {
    directory: PathBuf,
    default_language: String,
    template: Option<PathBuf>,
    assets: Vec<Pattern>,
}

impl Source {
    fn load(&self) -> Result<Site> {
        Site::load(
            &self.directory,
            &self.default_language,
            self.template.as_deref(),
            &self.assets,
        )
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = Config::load(args.config.as_deref())?;
    let source = Source {
        directory: args
            .source
            .or(config.source)
            .unwrap_or_else(|| "content".into()),
        default_language: args
            .default_language
            .or(config.default_language)
            .unwrap_or_else(|| "en".into()),
        template: args.template.or(config.template),
        assets: config
            .assets
            .iter()
            .map(|pattern| {
                Pattern::new(pattern).with_context(|| format!("invalid asset pattern {pattern:?}"))
            })
            .collect::<Result<_>>()?,
    };
    match args.command {
        Command::Build { output } => {
            let output = output
                .or(config.output)
                .unwrap_or_else(|| "target/ryllis-site".into());
            let count = source.load()?.write(&output)?;
            println!("Wrote {count} pages to {}", output.display());
        }
        Command::Preview { bind } => {
            let bind = bind
                .or(config.bind)
                .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 3000)));
            source.load()?;
            let listener = tokio::net::TcpListener::bind(bind)
                .await
                .with_context(|| format!("binding to {bind}"))?;
            println!("Preview at http://{}", listener.local_addr()?);
            let app = Router::new().fallback(get(preview)).with_state(source);
            axum::serve(listener, app).await?;
        }
        Command::Check => {
            let site = source.load()?;
            for page in site.missing_translations() {
                eprintln!(
                    "{} is present in {} and missing in {}",
                    page.path.display(),
                    page.present.join(", "),
                    page.missing.join(", "),
                );
            }
            let broken = site.broken_links();
            for link in &broken {
                eprintln!("{link}");
            }
            if !broken.is_empty() {
                bail!("{} broken internal link(s)", broken.len());
            }
            println!("All internal page links resolve.");
        }
    }
    Ok(())
}

async fn preview(State(source): State<Source>, uri: Uri) -> Response {
    let response = tokio::task::spawn_blocking(move || source.load()?.preview(uri.path()))
        .await
        .context("loading preview source")
        .and_then(|result| result);
    match response {
        Ok(Some((mime, bytes))) => ([(header::CONTENT_TYPE, mime)], bytes).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Page not found").into_response(),
        Err(error) => {
            eprintln!("{error:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Could not load the site").into_response()
        }
    }
}
