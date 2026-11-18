mod site;

use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result, bail};
use axum::{
    Router,
    extract::State,
    http::{StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use clap::{Parser, Subcommand};
use site::Site;

#[derive(Parser)]
#[command(about = "Build and preview a multilingual Markdown site")]
struct Args {
    #[arg(long, global = true, default_value = "content")]
    source: PathBuf,
    #[arg(long, global = true, default_value = "en")]
    default_language: String,
    #[arg(long, global = true)]
    template: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write the generated HTML to a new directory.
    Build {
        #[arg(long, default_value = "target/ryllis-site")]
        output: PathBuf,
    },
    /// Preview the site, reloading source files when requested.
    Preview {
        #[arg(long, default_value = "127.0.0.1:3000")]
        bind: SocketAddr,
    },
    /// Report internal Markdown links whose destination has no generated page.
    Check,
}

#[derive(Clone)]
struct Source {
    directory: PathBuf,
    default_language: String,
    template: Option<PathBuf>,
}

impl Source {
    fn load(&self) -> Result<Site> {
        Site::load(
            &self.directory,
            &self.default_language,
            self.template.as_deref(),
        )
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let source = Source {
        directory: args.source,
        default_language: args.default_language,
        template: args.template,
    };
    match args.command {
        Command::Build { output } => {
            let count = source.load()?.write(&output)?;
            println!("Wrote {count} pages to {}", output.display());
        }
        Command::Preview { bind } => {
            source.load()?;
            let listener = tokio::net::TcpListener::bind(bind)
                .await
                .with_context(|| format!("binding to {bind}"))?;
            println!("Preview at http://{}", listener.local_addr()?);
            let app = Router::new().fallback(get(preview)).with_state(source);
            axum::serve(listener, app).await?;
        }
        Command::Check => {
            let broken = source.load()?.broken_links();
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
    let site = tokio::task::spawn_blocking(move || source.load())
        .await
        .context("loading preview source")
        .and_then(|result| result);
    match site {
        Ok(site) => match site.page(uri.path()) {
            Some(html) => Html(html.to_owned()).into_response(),
            None => (StatusCode::NOT_FOUND, "Page not found").into_response(),
        },
        Err(error) => {
            eprintln!("{error:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Could not load the site").into_response()
        }
    }
}
