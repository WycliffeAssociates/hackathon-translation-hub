//! Configuration, from the environment. Every value has a development default
//! that works from the repo root with `dev-data/` in place.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    /// `HUB_REPO_ROOT`: Forgejo's repository directory, read-only.
    pub repo_root: PathBuf,
    /// `HUB_DB`: the hub's SQLite file.
    pub db: PathBuf,
    /// Where Forgejo's repo list comes from.
    pub forgejo: ForgejoSource,
    /// `HUB_BASE_URL`: the public WACS URL, for clone links.
    pub base_url: String,
    /// `HUB_LISTEN`.
    pub listen: SocketAddr,
    /// `HUB_POLL_SECS`: how often the soft poll looks for changed repos.
    pub poll_every: Duration,
    /// `HUB_CATALOG_SECS`: how often the public data API copy is refreshed.
    pub catalog_every: Duration,
    /// `HUB_CATALOG_URL`: the public data API; `off` disables the fetch.
    pub catalog_url: Option<String>,
    /// `HUB_WEB_DIR`: the built web app, served at `/`, when set.
    pub web_dir: Option<PathBuf>,
}

#[derive(Clone)]
pub enum ForgejoSource {
    /// `HUB_FORGEJO_DUMP`: a `mysqldump` of `repository` (development).
    Dump(PathBuf),
    /// `HUB_FORGEJO_DB`: `mysql://user:pass@host/gitea`, read-only (production;
    /// needs the `mariadb` feature).
    #[cfg_attr(not(feature = "mariadb"), allow(dead_code))]
    MariaDb(String),
}

/// The MariaDB URL holds a password, and the config is logged at startup.
impl std::fmt::Debug for ForgejoSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dump(path) => f.debug_tuple("Dump").field(path).finish(),
            Self::MariaDb(_) => f.write_str("MariaDb(<redacted>)"),
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let secs = |k: &str, default: u64| {
            Duration::from_secs(var(k).and_then(|v| v.parse().ok()).unwrap_or(default))
        };
        let forgejo = match (var("HUB_FORGEJO_DB"), var("HUB_FORGEJO_DUMP")) {
            (Some(url), _) => ForgejoSource::MariaDb(url),
            (None, Some(dump)) => ForgejoSource::Dump(dump.into()),
            (None, None) => ForgejoSource::Dump("dev-data/sql/repository.sql.gz".into()),
        };
        let catalog_url = match var("HUB_CATALOG_URL").as_deref() {
            Some("off") => None,
            Some(url) => Some(url.to_owned()),
            None => Some("https://api.bibleineverylanguage.org/v1/graphql".to_owned()),
        };
        Self {
            repo_root: var("HUB_REPO_ROOT")
                .unwrap_or("dev-data/repositories".into())
                .into(),
            db: var("HUB_DB").unwrap_or("hub.sqlite".into()).into(),
            forgejo,
            base_url: var("HUB_BASE_URL")
                .unwrap_or("https://content.bibletranslationtools.org".into()),
            listen: var("HUB_LISTEN")
                .unwrap_or("127.0.0.1:8787".into())
                .parse()
                .expect("HUB_LISTEN must be host:port"),
            poll_every: secs("HUB_POLL_SECS", 300),
            catalog_every: secs("HUB_CATALOG_SECS", 3600),
            catalog_url,
            web_dir: var("HUB_WEB_DIR").map(Into::into),
        }
    }
}
