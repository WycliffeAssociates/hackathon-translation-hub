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

tokio::task_local! {
    /// Forgejo API calls made by the current request, and their total time
    /// in ms: the wide event reports them (`forgejo_calls`, `forgejo_ms`).
    pub static UPSTREAM: std::cell::Cell<(u32, f64)>;
}

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
    /// A Forgejo site admin (the person's own /user says so).
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub avatar_url: String,
}

/// A person's OAuth tokens from Forgejo.
pub struct Tokens {
    /// Used once, at sign-in, to ask Forgejo who the person is; never stored.
    pub access: String,
}

/// A release, as the hub shows it.
#[derive(Debug, serde::Serialize, Deserialize, utoipa::ToSchema)]
pub struct Release {
    #[serde(default)]
    pub id: i64,
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
    /// Files attached to the release (`SHA256SUMS`, `release.json`).
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, serde::Serialize, Deserialize, utoipa::ToSchema)]
pub struct ReleaseAsset {
    pub name: String,
    #[serde(default)]
    pub browser_download_url: String,
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
        let started = std::time::Instant::now();
        let out = self.send_untimed(what, req).await;
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        // Outside a request (none today) there is nothing to add to.
        let _ = UPSTREAM.try_with(|u| {
            let (n, t) = u.get();
            u.set((n + 1, t + ms));
        });
        out
    }

    async fn send_untimed(
        &self,
        what: &str,
        req: reqwest::RequestBuilder,
    ) -> Result<Option<Value>> {
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

    // --- a person's home --------------------------------------------------

    /// The repos a person starred (Forgejo's own pin), `owner/name`, newest first.
    /// Public: the service account reads what anyone may.
    pub async fn starred(&self, username: &str) -> Result<Vec<String>> {
        let v = self
            .get(&format!("/users/{username}/starred?limit=50"))
            .await?;
        Ok(v.as_ref()
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|r| r["full_name"].as_str().map(str::to_owned))
            .collect())
    }

    /// The organisations a person belongs to (their public memberships).
    pub async fn orgs(&self, username: &str) -> Result<Vec<String>> {
        let v = self
            .get(&format!("/users/{username}/orgs?limit=50"))
            .await?;
        Ok(v.as_ref()
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|o| o["username"].as_str().map(str::to_owned))
            .collect())
    }

    // --- sign-in -----------------------------------------------------------

    /// Trades an OAuth code for the user's access token.
    pub async fn exchange_code(&self, p: &Publishing, code: &str) -> Result<Tokens> {
        self.token_grant(p, &[("code", code), ("grant_type", "authorization_code")])
            .await
    }

    async fn token_grant(&self, p: &Publishing, grant: &[(&str, &str)]) -> Result<Tokens> {
        let redirect = format!("{}/v1/auth/callback", p.public_url);
        let mut form: Vec<(&str, &str)> = vec![
            ("client_id", p.oauth_client_id.as_str()),
            ("client_secret", p.oauth_client_secret.as_str()),
            ("redirect_uri", redirect.as_str()),
        ];
        form.extend_from_slice(grant);
        let res = self
            .send(
                "oauth token",
                self.http
                    .post(format!("{}/login/oauth/access_token", self.base))
                    .form(&form),
            )
            .await?
            .unwrap_or(Value::Null);
        let access = res["access_token"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| ForgejoError::Refused("Forgejo gave no access token".into()))?;
        Ok(Tokens { access })
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
    /// `init`: start it with a README commit (books pulled in one by one);
    /// without, it is empty and waits for a push (a consolidated history).
    pub async fn create_repo(&self, name: &str, description: &str, init: bool) -> Result<Value> {
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
                    "auto_init": init,
                    "readme": "Default",
                    "default_branch": "master",
                })),
        )
        .await?
        .ok_or_else(|| ForgejoError::Refused(format!("no org {org}")))
    }

    /// The service account's login (for git's basic auth, with the token as password).
    pub async fn whoami(&self) -> Result<String> {
        let v = self
            .get("/user")
            .await?
            .ok_or_else(|| ForgejoError::Refused("the token has no user".into()))?;
        Ok(v["login"].as_str().unwrap_or_default().to_owned())
    }

    /// Push `branch` of the local repository at `local` to an official repo,
    /// as the service account. Through Forgejo's git endpoint, so its hooks run
    /// and its database follows, like any push. Never forced. Blocking.
    pub fn push(
        &self,
        login: &str,
        local: &std::path::Path,
        owner: &str,
        name: &str,
        branch: &str,
    ) -> Result<()> {
        self.may_write(owner)?;
        let refused = |e: git2::Error| ForgejoError::Refused(format!("push: {}", e.message()));
        let repo = git2::Repository::open(local).map_err(refused)?;
        let mut remote = repo
            .remote_anonymous(&format!("{}/{owner}/{name}.git", self.base))
            .map_err(refused)?;
        let mut callbacks = git2::RemoteCallbacks::new();
        let (login, token) = (login.to_owned(), self.token.clone());
        callbacks.credentials(move |_, _, _| git2::Cred::userpass_plaintext(&login, &token));
        let rejected = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
        let r = rejected.clone();
        callbacks.push_update_reference(move |refname, status| {
            if let Some(s) = status {
                *r.lock().expect("push status") = Some(format!("{refname}: {s}"));
            }
            Ok(())
        });
        let mut opts = git2::PushOptions::new();
        opts.remote_callbacks(callbacks);
        remote
            .push(
                &[format!("refs/heads/{branch}:refs/heads/{branch}")],
                Some(&mut opts),
            )
            .map_err(refused)?;
        if let Some(why) = rejected.lock().expect("push status").take() {
            return Err(ForgejoError::Refused(format!("push rejected: {why}")));
        }
        Ok(())
    }

    /// Delete an official repo the hub just created and could not fill (a
    /// failed push), so a retry starts clean. Refused unless it is empty.
    pub async fn delete_empty(&self, owner: &str, name: &str) -> Result<()> {
        self.may_write(owner)?;
        let empty = self
            .get(&format!("/repos/{owner}/{name}"))
            .await?
            .is_some_and(|r| r["empty"].as_bool() == Some(true));
        if !empty {
            return Err(ForgejoError::Refused(format!(
                "{owner}/{name} is not empty; not deleting it"
            )));
        }
        self.send(
            "delete",
            self.http
                .delete(self.url(&format!("/repos/{owner}/{name}")))
                .bearer_auth(&self.token),
        )
        .await?;
        Ok(())
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

    /// Attach a file to a release (Forgejo's release attachments). The
    /// multipart body is built here so reqwest needs no extra feature.
    pub async fn attach(
        &self,
        owner: &str,
        name: &str,
        release: i64,
        file: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<()> {
        self.may_write(owner)?;
        let boundary = format!("hub-{:016x}", rand_u64());
        let mut body = Vec::with_capacity(bytes.len() + 256);
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"attachment\"; filename=\"{file}\"\r\nContent-Type: {content_type}\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        self.send(
            "release attachment",
            self.http
                .post(self.url(&format!(
                    "/repos/{owner}/{name}/releases/{release}/assets?name={file}"
                )))
                .bearer_auth(&self.token)
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(body),
        )
        .await?;
        Ok(())
    }
}

/// A boundary that will not occur in the files (time and address bits; no rand crate).
fn rand_u64() -> u64 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    t ^ (&t as *const u64 as u64).rotate_left(17)
}
