//! A full refresh against the live API, with counts and timings.
//!
//! ```sh
//! RUST_LOG=info REPO_ROOT=$PWD/dev-data/repositories cargo run --release -p hub-biel --example fetch
//! ```
//!
//! With `REPO_ROOT`, also reports how many catalog repos sit on disk there.

use std::collections::HashSet;
use std::time::Instant;

use hub_biel::{Client, RepoPath};
use hub_domain::Status;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();
    let client = Client::new();

    let t = Instant::now();
    let languages = client.languages().await?;
    println!(
        "languages: {} ({} gateway, {} rtl) in {:.1?}",
        languages.len(),
        languages.iter().filter(|l| l.gateway).count(),
        languages
            .iter()
            .filter(|l| l.direction.as_deref() == Some("rtl"))
            .count(),
        t.elapsed()
    );

    let t = Instant::now();
    let contents = client.contents().await?;
    let count =
        |f: &dyn Fn(&hub_biel::CatalogContent) -> bool| contents.iter().filter(|c| f(c)).count();
    println!("contents: {} in {:.1?}", contents.len(), t.elapsed());
    println!(
        "  primary {} · primary+shown {} · with WACS repo {} · no language {} · with books {}",
        count(&|c| c.status == Some(Status::Primary)),
        count(&|c| c.status == Some(Status::Primary) && c.show_on_biel),
        count(&|c| c.repo.is_some()),
        count(&|c| c.language.is_none()),
        count(&|c| !c.books.is_empty()),
    );

    if let Ok(root) = std::env::var("REPO_ROOT") {
        let mut local = HashSet::new();
        for owner in std::fs::read_dir(&root)? {
            let owner = owner?;
            if !owner.file_type()?.is_dir() {
                continue;
            }
            for repo in std::fs::read_dir(owner.path())? {
                let file = repo?.file_name().to_string_lossy().into_owned();
                if let Some(name) = file.strip_suffix(".git") {
                    local.insert(RepoPath {
                        owner: owner.file_name().to_string_lossy().into_owned(),
                        name: name.to_owned(),
                    });
                }
            }
        }
        let catalog: HashSet<_> = contents.iter().filter_map(|c| c.repo.clone()).collect();
        let matched = catalog.intersection(&local).count();
        println!(
            "dev repos with a catalog row: {matched}/{} · catalog WACS repos on disk: {matched}/{}",
            local.len(),
            catalog.len()
        );
    }
    Ok(())
}
