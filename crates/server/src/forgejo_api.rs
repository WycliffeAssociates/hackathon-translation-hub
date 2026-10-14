//! Forgejo's HTTP API: sign-in (OAuth token exchange, who the user is, team
//! membership) and the hub's only writes, which go to the official org as the
//! service account. A proposal.
//!
//! The hub READS repos off disk and Forgejo's DB (see `hub-ingest`); this
//! module is the one place that talks to Forgejo over HTTP, and the one place
//! that writes. Writes go through Forgejo so its DB, hooks and repo folder stay
//! consistent; nothing here touches the repo folder.
//!
//! The guard: a client that can write is built only for a host in
//! `HUB_WRITE_HOSTS` that is not production WACS, and every write names its
//! owner, which must be the official org. Both are checked here, at the call,
//! not by the routes that ask.

use std::time::Duration;

use base64::Engine as _;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::Publishing;

/// Production WACS. Refused as a write host whatever the config says.
const PRODUCTION_HOSTS: &[&str] = &["content.bibletranslationtools.org", "git.door43.org"];

#[derive(Debug, thiserror::Error)]
pub enum ForgejoError {
    #[error("refused: {0}")]
    Refused(String),
    #[error("forgejo {status} on {what}: {body}")]
    Status {
        status: u16,
        what: String,
        body: String,
    },
    #[error("forgejo unreachable: {0}")]
    Http(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, ForgejoError>;

pub struct Forgejo {
    base: String,
    token: String,
    official_org: String,
    http: reqwest::Client,
}

/// Who signed in, from `/api/v1/user`.
#[derive(Debug, Deserialize)]
pub struct User {
    pub login: String,
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub avatar_url: String,
}

/// A release, as the hub shows it.
#[derive(Debug, serde::Serialize, Deserialize, utoipa::ToSchema)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub target_commitish: String,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub html_url: String,
}

/// One file in a multi-file commit.
pub struct FileChange<'a> {
    pub path: &'a str,
    pub text: &'a str,
    /// The blob sha of the file being replaced; `None` creates it.
    pub replaces: Option<String>,
}

impl Forgejo {
    /// The client, if the configured host may be written to.
    pub fn new(p: &Publishing) -> Result<Self> {
        let host = p
            .forgejo_url
            .split("://")
            .nth(1)
            .unwrap_or(&p.forgejo_url)
            .split(['/', ':'])
            .next()
            .unwrap_or("")
            .to_lowercase();
        if PRODUCTION_HOSTS.contains(&host.as_str()) {
            return Err(ForgejoError::Refused(format!(
                "{host} is production WACS; the hub never writes there"
            )));
        }
        if !p.write_hosts.contains(&host) {
            return Err(ForgejoError::Refused(format!(
                "{host} is not in HUB_WRITE_HOSTS"
            )));
        }
        Ok(Self {
            base: p.forgejo_url.clone(),
            token: p.token.clone(),
            official_org: p.official_org.clone(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .user_agent("translation-hub")
                .build()?,
        })
    }

    /// Every write names its owner; only the official org passes.
    fn may_write(&self, owner: &str) -> Result<()> {
        if owner.eq_ignore_ascii_case(&self.official_org) {
            Ok(())
        } else {
            Err(ForgejoError::Refused(format!(
                "{owner} is not the official org {}",
                self.official_org
            )))
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.base)
    }

    async fn send(&self, what: &str, req: reqwest::RequestBuilder) -> Result<Option<Value>> {
        let res = req.send().await?;
        let status = res.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let body = res.text().await?;
        if !status.is_success() {
            return Err(ForgejoError::Status {
                status: status.as_u16(),
                what: what.to_owned(),
                body: body.chars().take(400).collect(),
            });
        }
        Ok(Some(if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&body).unwrap_or(Value::Null)
        }))
    }

    async fn get(&self, path: &str) -> Result<Option<Value>> {
        self.send(path, self.http.get(self.url(path)).bearer_auth(&self.token))
            .await
    }

    // --- sign-in -----------------------------------------------------------

    /// Trades an OAuth code for the user's access token.
    pub async fn exchange_code(&self, p: &Publishing, code: &str) -> Result<String> {
        let res = self
            .send(
                "oauth token",
                self.http
                    .post(format!("{}/login/oauth/access_token", self.base))
                    .form(&[
                        ("client_id", p.oauth_client_id.as_str()),
                        ("client_secret", p.oauth_client_secret.as_str()),
                        ("code", code),
                        ("grant_type", "authorization_code"),
                        (
                            "redirect_uri",
                            &format!("{}/v1/auth/callback", p.public_url),
                        ),
                    ]),
            )
            .await?
            .unwrap_or(Value::Null);
        res["access_token"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| ForgejoError::Refused("Forgejo gave no access token".into()))
    }

    /// Who an access token belongs to.
    pub async fn user(&self, access_token: &str) -> Result<User> {
        let v = self
            .send(
                "user",
                self.http.get(self.url("/user")).bearer_auth(access_token),
            )
            .await?
            .ok_or_else(|| ForgejoError::Refused("no such user".into()))?;
        serde_json::from_value(v).map_err(|e| ForgejoError::Refused(e.to_string()))
    }

    /// Is `username` in the official org's `team`? Read as the service account.
    /// (The teams list, not `teams/search`: Forgejo 15's search answered empty
    /// for a team that exists.)
    pub async fn in_team(&self, team: &str, username: &str) -> Result<bool> {
        let org = &self.official_org;
        let Some(teams) = self.get(&format!("/orgs/{org}/teams?limit=50")).await? else {
            return Ok(false);
        };
        let Some(id) = teams
            .as_array()
            .and_then(|ts| {
                ts.iter().find(|t| {
                    t["name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(team))
                })
            })
            .and_then(|t| t["id"].as_i64())
        else {
            return Ok(false);
        };
        Ok(self
            .get(&format!("/teams/{id}/members/{username}"))
            .await?
            .is_some())
    }

    // --- reads -------------------------------------------------------------

    pub async fn repo_exists(&self, owner: &str, name: &str) -> Result<bool> {
        Ok(self.get(&format!("/repos/{owner}/{name}")).await?.is_some())
    }

    pub async fn releases(&self, owner: &str, name: &str) -> Result<Vec<Release>> {
        Ok(
            match self
                .get(&format!("/repos/{owner}/{name}/releases?limit=50"))
                .await?
            {
                Some(v) => serde_json::from_value(v).unwrap_or_default(),
                None => Vec::new(),
            },
        )
    }

    /// The blob sha of a file on the default branch, if it is there.
    pub async fn file_sha(&self, owner: &str, name: &str, path: &str) -> Result<Option<String>> {
        Ok(self
            .get(&format!("/repos/{owner}/{name}/contents/{path}"))
            .await?
            .and_then(|v| v["sha"].as_str().map(str::to_owned)))
    }

    /// A text file on the default branch: its blob sha and contents.
    pub async fn file_text(
        &self,
        owner: &str,
        name: &str,
        path: &str,
    ) -> Result<Option<(String, String)>> {
        let Some(v) = self
            .get(&format!("/repos/{owner}/{name}/contents/{path}"))
            .await?
        else {
            return Ok(None);
        };
        let sha = v["sha"].as_str().unwrap_or_default().to_owned();
        let raw: String = v["content"]
            .as_str()
            .unwrap_or_default()
            .split_whitespace()
            .collect();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(raw)
            .map_err(|e| ForgejoError::Refused(format!("{path}: {e}")))?;
        Ok(Some((sha, String::from_utf8_lossy(&bytes).into_owned())))
    }

    // --- writes (official org only) ----------------------------------------

    /// Adopt: fork `src` into the official org as `name`. A fork keeps the
    /// whole history and Forgejo shows where it came from.
    pub async fn fork(&self, src_owner: &str, src_name: &str, name: &str) -> Result<Value> {
        // The destination is the official org by construction; the source is only read.
        let org = self.official_org.clone();
        let forked = self
            .send(
                "fork",
                self.http
                    .post(self.url(&format!("/repos/{src_owner}/{src_name}/forks")))
                    .bearer_auth(&self.token)
                    .json(&json!({ "organization": org, "name": name })),
            )
            .await?
            .ok_or_else(|| {
                ForgejoError::Refused(format!("no repo {src_owner}/{src_name} to fork"))
            })?;
        // A fork gets only code and pull requests (Forgejo's
        // DEFAULT_FORK_REPO_UNITS), and an official repo publishes releases.
        self.send(
            "enable releases",
            self.http
                .patch(self.url(&format!("/repos/{org}/{name}")))
                .bearer_auth(&self.token)
                .json(&json!({ "has_releases": true, "has_issues": true })),
        )
        .await?;
        Ok(forked)
    }

    /// A new, empty official repo (one commit: a README), for books pulled in one by one.
    pub async fn create_repo(&self, name: &str, description: &str) -> Result<Value> {
        // Created in the official org by construction.
        let org = self.official_org.clone();
        self.send(
            "create repo",
            self.http
                .post(self.url(&format!("/orgs/{org}/repos")))
                .bearer_auth(&self.token)
                .json(&json!({
                    "name": name,
                    "description": description,
                    "private": false,
                    "auto_init": true,
                    "readme": "Default",
                    "default_branch": "master",
                })),
        )
        .await?
        .ok_or_else(|| ForgejoError::Refused(format!("no org {org}")))
    }

    /// One commit of one or more files to the default branch; returns the commit sha.
    pub async fn commit(
        &self,
        owner: &str,
        name: &str,
        message: &str,
        files: &[FileChange<'_>],
    ) -> Result<String> {
        self.may_write(owner)?;
        let b64 = base64::engine::general_purpose::STANDARD;
        let files: Vec<Value> = files
            .iter()
            .map(|f| {
                let mut v = json!({
                    "operation": if f.replaces.is_some() { "update" } else { "create" },
                    "path": f.path,
                    "content": b64.encode(f.text.as_bytes()),
                });
                if let Some(sha) = &f.replaces {
                    v["sha"] = json!(sha);
                }
                v
            })
            .collect();
        let v = self
            .send(
                "commit",
                self.http
                    .post(self.url(&format!("/repos/{owner}/{name}/contents")))
                    .bearer_auth(&self.token)
                    .json(&json!({ "message": message, "files": files })),
            )
            .await?
            .ok_or_else(|| ForgejoError::Refused(format!("no repo {owner}/{name}")))?;
        Ok(v["commit"]["sha"].as_str().unwrap_or_default().to_owned())
    }

    /// Publish: a release (and its tag) at the default branch's head.
    pub async fn create_release(
        &self,
        owner: &str,
        name: &str,
        tag: &str,
        title: &str,
        notes: &str,
    ) -> Result<Release> {
        self.may_write(owner)?;
        // The release goes on the default branch's head, named explicitly
        // rather than left to Forgejo's default.
        let branch = self
            .get(&format!("/repos/{owner}/{name}"))
            .await?
            .and_then(|r| r["default_branch"].as_str().map(str::to_owned))
            .unwrap_or_else(|| "master".into());
        let v = self
            .send(
                "release",
                self.http
                    .post(self.url(&format!("/repos/{owner}/{name}/releases")))
                    .bearer_auth(&self.token)
                    .json(&json!({
                        "tag_name": tag,
                        "target_commitish": branch,
                        "name": title,
                        "body": notes,
                        "draft": false,
                        "prerelease": false,
                    })),
            )
            .await?
            .ok_or_else(|| ForgejoError::Refused(format!("no repo {owner}/{name}")))?;
        serde_json::from_value(v).map_err(|e| ForgejoError::Refused(e.to_string()))
    }
}
