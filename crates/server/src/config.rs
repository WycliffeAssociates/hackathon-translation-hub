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
    /// `HUB_SEFER_URL`: Sefer on the Web, for the "Open in Sefer" link.
    pub sefer_url: String,
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
    /// Publishing (sign-in and writes through Forgejo's API), when configured.
    pub publishing: Option<Publishing>,
}

/// Sign-in with Forgejo, and the hub's writes to its official org. Off unless
/// `HUB_FORGEJO_URL` and `HUB_FORGEJO_TOKEN` are both set.
#[derive(Clone)]
pub struct Publishing {
    /// `HUB_FORGEJO_URL`: Forgejo as the hub reaches it (API, token exchange).
    pub forgejo_url: String,
    /// `HUB_FORGEJO_PUBLIC_URL`: Forgejo as a browser reaches it (the sign-in
    /// page); defaults to `forgejo_url`.
    pub forgejo_public_url: String,
    /// `HUB_FORGEJO_TOKEN`: the service account that writes, and reads team membership.
    pub token: String,
    /// `HUB_OFFICIAL_ORG` (`TextTranslation`): the only owner the hub writes to.
    pub official_org: String,
    /// `HUB_PUBLISHERS_TEAM` (`publishers`): who may adopt, pull and publish.
    /// `admins` makes Forgejo's site admins the publishers, with no team.
    pub publishers_team: String,
    /// `HUB_OAUTH_CLIENT_ID`, `HUB_OAUTH_CLIENT_SECRET`: the hub's OAuth app on Forgejo.
    pub oauth_client_id: String,
    pub oauth_client_secret: String,
    /// `HUB_PUBLIC_URL`: the hub as a browser reaches it, for the OAuth redirect.
    pub public_url: String,
    /// `HUB_WRITE_HOSTS`: hosts the hub may write to. Production WACS is never
    /// one of them, whatever this says.
    pub write_hosts: Vec<String>,
}

impl std::fmt::Debug for Publishing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Publishing")
            .field("forgejo_url", &self.forgejo_url)
            .field("official_org", &self.official_org)
            .field("publishers_team", &self.publishers_team)
            .field("public_url", &self.public_url)
            .field("write_hosts", &self.write_hosts)
            .finish_non_exhaustive()
    }
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
        let listen: SocketAddr = var("HUB_LISTEN")
            .unwrap_or("127.0.0.1:8787".into())
            .parse()
            .expect("HUB_LISTEN must be host:port");
        let publishing = match (var("HUB_FORGEJO_URL"), var("HUB_FORGEJO_TOKEN")) {
            (Some(url), Some(token)) => {
                let url = url.trim_end_matches('/').to_owned();
                Some(Publishing {
                    forgejo_public_url: var("HUB_FORGEJO_PUBLIC_URL")
                        .map_or_else(|| url.clone(), |u| u.trim_end_matches('/').to_owned()),
                    forgejo_url: url,
                    token,
                    official_org: var("HUB_OFFICIAL_ORG").unwrap_or("TextTranslation".into()),
                    publishers_team: var("HUB_PUBLISHERS_TEAM").unwrap_or("publishers".into()),
                    oauth_client_id: var("HUB_OAUTH_CLIENT_ID").unwrap_or_default(),
                    oauth_client_secret: var("HUB_OAUTH_CLIENT_SECRET").unwrap_or_default(),
                    public_url: var("HUB_PUBLIC_URL").map_or_else(
                        || format!("http://{listen}"),
                        |u| u.trim_end_matches('/').to_owned(),
                    ),
                    write_hosts: var("HUB_WRITE_HOSTS")
                        .unwrap_or("127.0.0.1,localhost,gitea,content.wacsdev.org".into())
                        .split(',')
                        .map(|h| h.trim().to_lowercase())
                        .filter(|h| !h.is_empty())
                        .collect(),
                })
            }
            _ => None,
        };
        Self {
            repo_root: var("HUB_REPO_ROOT")
                .unwrap_or("dev-data/repositories".into())
                .into(),
            db: var("HUB_DB").unwrap_or("hub.sqlite".into()).into(),
            forgejo,
            base_url: var("HUB_BASE_URL")
                .unwrap_or("https://content.bibletranslationtools.org".into()),
            sefer_url: var("HUB_SEFER_URL").unwrap_or("https://sefer-dev.bttdev.org".into()),
            listen,
            poll_every: secs("HUB_POLL_SECS", 300),
            catalog_every: secs("HUB_CATALOG_SECS", 3600),
            catalog_url,
            web_dir: var("HUB_WEB_DIR").map(Into::into),
            publishing,
        }
    }
}
