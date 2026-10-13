//! The soft poll: keep the store in step with the disk, Forgejo and the
//! public data API, without webhooks.
//!
//! Every `poll_every`:
//! 1. Read Forgejo's repo list (public repos only) into `repo`.
//! 2. Rescan every repo whose HEAD on disk moved, whose Forgejo `updated_unix`
//!    moved, or that was scanned with another scripture-kitchen tag. Reading
//!    HEAD is a couple of small file reads per repo, so checking all of them
//!    each round is cheap; the scans run in parallel on rayon.
//!
//! Every `catalog_every`: replace the copy of the public data API's languages
//! and catalog rows. Pages read the copy, never the API.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use hub_ingest::Scanner;
use hub_ingest::forgejo::{ForgejoRepo, from_dump};
use hub_store::{CatalogRow, LanguageRow, Store};
use rayon::prelude::*;
use tracing::{Instrument, info, info_span, warn};

use crate::config::{Config, ForgejoSource};

pub struct Poller {
    pub config: Config,
    pub store: Arc<Store>,
    pub scanner: Arc<Scanner>,
}

impl Poller {
    pub async fn run_repos(self: Arc<Self>) {
        let mut tick = tokio::time::interval(self.config.poll_every);
        loop {
            tick.tick().await;
            let me = self.clone();
            let span = info_span!("poll.repos");
            let result =
                tokio::task::spawn_blocking(move || span.in_scope(|| me.poll_repos())).await;
            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => warn!(error = %e, "poll failed"),
                Err(e) => warn!(error = %e, "poll panicked"),
            }
        }
    }

    pub async fn run_catalog(self: Arc<Self>) {
        let Some(url) = self.config.catalog_url.clone() else {
            info!("catalog fetch off (HUB_CATALOG_URL=off)");
            return;
        };
        let client = hub_biel::Client::with_endpoint(url, reqwest_client());
        let mut tick = tokio::time::interval(self.config.catalog_every);
        loop {
            tick.tick().await;
            if let Err(e) = self
                .refresh_catalog(&client)
                .instrument(info_span!("poll.catalog"))
                .await
            {
                warn!(error = %e, "catalog refresh failed; keeping the last copy");
            }
        }
    }

    fn poll_repos(&self) -> Result<(), String> {
        let started = Instant::now();
        let forgejo = self.forgejo_repos()?;
        let counts = self
            .store
            .sync_forgejo(&forgejo, |r| {
                self.scanner.repo_path(&r.owner, &r.name).is_dir()
            })
            .map_err(|e| e.to_string())?;
        let tag = hub_checks::KITCHEN_TAG;
        let stale: Vec<_> = self
            .store
            .scan_marks()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|m| {
                let disk =
                    hub_repo_read::head(&self.scanner.repo_path(&m.repo.owner, &m.repo.name))
                        .ok()
                        .flatten()
                        .map(|h| h.sha);
                m.scanned_tag.as_deref() != Some(tag)
                    || m.scanned_updated != Some(m.updated)
                    || disk != m.repo.head_sha
            })
            .map(|m| m.repo)
            .collect();
        let scans: Vec<_> = stale
            .par_iter()
            .map(|r| (r.id, self.scanner.scan(&r.owner, &r.name)))
            .collect();
        for (id, scan) in &scans {
            if let Err(e) = self.store.write_scan(*id, scan, tag) {
                warn!(repo = %format!("{}/{}", scan.owner, scan.name), error = %e, "store write failed");
            }
        }
        let scanned_at = started.elapsed();
        let proofread = self.proofread();
        let _ = self.store.set_state("polled_unix", &unix_now().to_string());
        // The poll's wide event: one line with everything a slow round needs.
        info!(
            event = "poll",
            public = counts.public,
            removed = counts.removed,
            rescanned = scans.len(),
            scan_failures = scans.iter().filter(|(_, s)| s.error.is_some()).count(),
            scan_ms = scanned_at.as_millis() as u64,
            proofread = proofread.runs,
            proof_failures = proofread.failures,
            proof_ms = proofread.millis,
            proof_paired = proofread.paired,
            proof_sources = proofread.sources,
            proof_source_ms = proofread.source_ms,
            proof_slowest_ms = proofread.slowest_ms,
            proof_slowest = %proofread.slowest,
            duration_ms = started.elapsed().as_millis() as u64,
            "poll done"
        );
        Ok(())
    }

    /// Proofreads every repo whose HEAD, kitchen tag, or source text moved
    /// since its last run, in parallel (sous itself is single-threaded per
    /// corpus). Each source text due repos declare is read and prepared once.
    fn proofread(&self) -> ProofRound {
        let started = Instant::now();
        let marks = match self.store.proof_marks() {
            Ok(marks) => marks,
            Err(e) => {
                warn!(error = %e, "reading proof marks failed");
                return ProofRound::default();
            }
        };
        // Each source's HEAD now: a couple of file reads per distinct source.
        let mut source_heads: HashMap<&str, Option<String>> = HashMap::new();
        for m in &marks {
            if let Some(repo) = m.source_repo.as_deref() {
                source_heads.entry(repo).or_insert_with(|| {
                    let (owner, name) = repo.split_once('/')?;
                    hub_repo_read::head(&self.scanner.repo_path(owner, name))
                        .ok()
                        .flatten()
                        .map(|h| h.sha)
                });
            }
        }
        let due: Vec<_> = marks
            .iter()
            .filter(|m| {
                let source_head = m
                    .source_repo
                    .as_deref()
                    .and_then(|r| source_heads[r].clone());
                m.head_sha != m.repo.head_sha
                    || m.sk_tag.as_deref() != Some(hub_proof::KITCHEN_TAG)
                    || m.proof_source_repo != m.source_repo
                    || m.proof_source_sha != source_head
            })
            .collect();

        let source_started = Instant::now();
        let mut wanted: Vec<&str> = due
            .iter()
            .filter_map(|m| m.source_repo.as_deref())
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        let sources: HashMap<&str, crate::proof::SourceText> = wanted
            .par_iter()
            .filter_map(|repo| Some((*repo, crate::proof::prepare_source(&self.scanner, repo)?)))
            .collect();
        let source_ms = source_started.elapsed().as_millis() as u64;

        let runs: Vec<_> = due
            .par_iter()
            .filter_map(|m| {
                let source = m.source_repo.as_deref().and_then(|r| sources.get(r));
                crate::proof::run(&self.scanner, &m.repo, m.language.as_deref(), source)
                    .map(|run| (format!("{}/{}", m.repo.owner, m.repo.name), run))
            })
            .collect();
        let mut round = ProofRound {
            runs: runs.len(),
            sources: sources.len(),
            source_ms,
            ..ProofRound::default()
        };
        for (repo, run) in &runs {
            round.failures += usize::from(run.error.is_some());
            round.paired += usize::from(run.source_repo.is_some());
            if run.millis as u64 > round.slowest_ms {
                round.slowest_ms = run.millis as u64;
                round.slowest = repo.clone();
            }
            if let Err(e) = self.store.write_proof(run, hub_proof::KITCHEN_TAG) {
                warn!(repo = %repo, error = %e, "storing proof failed");
            }
        }
        round.millis = started.elapsed().as_millis() as u64;
        round
    }

    fn forgejo_repos(&self) -> Result<Vec<ForgejoRepo>, String> {
        match &self.config.forgejo {
            ForgejoSource::Dump(path) => from_dump(path).map_err(|e| e.to_string()),
            #[cfg(feature = "mariadb")]
            ForgejoSource::MariaDb(url) => {
                hub_ingest::forgejo::from_mariadb(url).map_err(|e| e.to_string())
            }
            #[cfg(not(feature = "mariadb"))]
            ForgejoSource::MariaDb(_) => {
                Err("HUB_FORGEJO_DB needs a build with --features mariadb".into())
            }
        }
    }

    async fn refresh_catalog(&self, client: &hub_biel::Client) -> Result<(), String> {
        let started = Instant::now();
        let languages = client.languages().await.map_err(|e| e.to_string())?;
        let contents = client.contents().await.map_err(|e| e.to_string())?;
        let languages: Vec<LanguageRow> = languages
            .into_iter()
            .map(|l| LanguageRow {
                ietf: l.ietf,
                name: l.name,
                english_name: l.english_name,
                direction: l.direction,
                region: l.region,
                gateway: Some(l.gateway),
                alternate_names: l.alternate_names,
            })
            .collect();
        let catalog: Vec<CatalogRow> = contents
            .into_iter()
            .map(|c| CatalogRow {
                id: c.id,
                owner: c.repo.as_ref().map(|r| r.owner.clone()),
                name: c.repo.as_ref().map(|r| r.name.clone()),
                language: c.language,
                resource_type: c.resource_type,
                track: c.track,
                domain: c.domain,
                status: c.status.map(|s| format!("{s:?}")),
                show_on_biel: c.show_on_biel,
                modified_on: c.modified_on,
                books: c.books,
            })
            .collect();
        let (n_lang, n_cat) = (languages.len(), catalog.len());
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            store
                .replace_languages(&languages)
                .map_err(|e| e.to_string())?;
            store.replace_catalog(&catalog).map_err(|e| e.to_string())?;
            store
                .set_state("catalog_unix", &unix_now().to_string())
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        info!(
            event = "catalog",
            duration_ms = started.elapsed().as_millis() as u64,
            languages = n_lang,
            catalog_rows = n_cat,
            "catalog refreshed"
        );
        Ok(())
    }
}

#[derive(Default)]
struct ProofRound {
    runs: usize,
    /// Runs that compared against a source text.
    paired: usize,
    /// Source texts prepared this round, and the time it took.
    sources: usize,
    source_ms: u64,
    failures: usize,
    millis: u64,
    slowest_ms: u64,
    slowest: String,
}

fn reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .gzip(true)
        .user_agent(concat!("translation-hub/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("http client")
}

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
