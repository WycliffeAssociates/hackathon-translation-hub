//! hub-repo-read: read bare git repos straight off disk, strictly read-only.
//!
//! Forgejo keeps every repo at `<root>/<owner>/<repo>.git` (both lowercase on
//! disk). In production that tree is mounted read-only, so nothing in here may
//! create or modify a file: no index, no lock files, no fetch, no gc.
//!
//! There are two halves:
//!
//! - [`list_repos`] and [`head`] work on plain files only. Reading `HEAD` and
//!   the ref it points at by hand is roughly twice as fast as asking libgit2,
//!   and it is the one thing we do for every repo on every scan.
//! - [`RepoReader`] wraps a `git2::Repository` for trees and blobs.
//!   `git2::Repository` is `Send` but not `Sync`, so open one reader per repo
//!   inside whatever closure processes that repo (e.g. a rayon `map`), and
//!   drop it when done. Opening is cheap.
//!
//! ```no_run
//! use hub_repo_read::{list_repos, head, RepoReader};
//! for repo in list_repos("/data/git/repositories".as_ref()).unwrap() {
//!     let Some(h) = head(&repo.path).unwrap() else { continue }; // empty repo
//!     let reader = RepoReader::open(&repo.path).unwrap();
//!     for entry in reader.root_files(&h.sha).unwrap() {
//!         println!("{}/{}: {}", repo.owner, repo.name, entry.path);
//!     }
//! }
//! ```

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use git2::Oid;
use git2::{ObjectType, Repository, TreeWalkMode, TreeWalkResult};

/// Everything that can go wrong while reading a repo.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A plain file read (directory listing, `HEAD`, a ref file) failed.
    #[error("reading {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    /// A ref file had content we could not make sense of.
    #[error("bad ref in {path}: {detail}")]
    BadRef { path: PathBuf, detail: String },
    /// A sha string the caller passed was not 40 hex chars (or a known prefix).
    #[error("bad object id {0:?}")]
    BadOid(String),
    /// libgit2 failed (missing object, corrupt pack, not a repo, ...).
    #[error(transparent)]
    Git(#[from] git2::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

// ---------------------------------------------------------------------------
// Listing repos
// ---------------------------------------------------------------------------

/// One repo found on disk. `owner` and `name` are taken from the directory
/// names (`name` without the `.git` suffix), so they are whatever case
/// Forgejo wrote, which is lowercase.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoId {
    pub owner: String,
    pub name: String,
    pub path: PathBuf,
}

/// Every `<owner>/<name>.git` directory directly under `root`, sorted by
/// owner then name. Anything else (stray files, dirs without `.git`) is
/// skipped. Unreadable owner directories are an error rather than silently
/// dropped, so a permissions problem shows up instead of a short list.
pub fn list_repos(root: &Path) -> Result<Vec<RepoId>> {
    let mut out = Vec::new();
    for owner in fs::read_dir(root).map_err(io_err(root))? {
        let owner = owner.map_err(io_err(root))?;
        if !owner.file_type().map_err(io_err(&owner.path()))?.is_dir() {
            continue;
        }
        let owner_path = owner.path();
        let Some(owner_name) = owner.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        for repo in fs::read_dir(&owner_path).map_err(io_err(&owner_path))? {
            let repo = repo.map_err(io_err(&owner_path))?;
            let file_name = repo.file_name();
            let Some(name) = file_name.to_str().and_then(|n| n.strip_suffix(".git")) else {
                continue;
            };
            if !repo.file_type().map_err(io_err(&repo.path()))?.is_dir() {
                continue;
            }
            out.push(RepoId {
                owner: owner_name.clone(),
                name: name.to_owned(),
                path: repo.path(),
            });
        }
    }
    out.sort();
    Ok(out)
}

// ---------------------------------------------------------------------------
// HEAD, by hand
// ---------------------------------------------------------------------------

/// Where a repo's HEAD points. `branch` is the short branch name (`master`,
/// `main`, `scribe-main`, ...) or `None` when HEAD is detached (a raw sha).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub branch: Option<String>,
    /// Full 40-char hex commit id.
    pub sha: String,
}

/// Resolve HEAD using only plain file reads.
///
/// Returns `Ok(None)` for an empty repo, i.e. HEAD names a branch that has
/// no commit yet (no loose ref file and no `packed-refs` entry).
///
/// Handles `ref: refs/heads/x` (following symbolic refs a few hops deep),
/// a loose ref file, `packed-refs` (ignoring the `^<sha>` peeled lines that
/// follow annotated tags), and a detached HEAD containing a sha.
pub fn head(repo: &Path) -> Result<Option<Head>> {
    let head_path = repo.join("HEAD");
    let content = fs::read_to_string(&head_path).map_err(io_err(&head_path))?;
    let content = content.trim();

    let Some(target) = content.strip_prefix("ref:") else {
        // Detached HEAD: the file is the sha itself.
        return parse_sha(content, &head_path).map(|sha| Some(Head { branch: None, sha }));
    };
    let mut refname = target.trim().to_owned();
    let branch = refname.strip_prefix("refs/heads/").map(str::to_owned);

    // A ref can itself be symbolic; git caps the chain at 5, so do we.
    for _ in 0..5 {
        match read_ref(repo, &refname)? {
            None => return Ok(None),
            Some(RefValue::Sha(sha)) => return Ok(Some(Head { branch, sha })),
            Some(RefValue::Symbolic(next)) => refname = next,
        }
    }
    Err(Error::BadRef {
        path: head_path,
        detail: "symbolic ref chain too deep".into(),
    })
}

enum RefValue {
    Sha(String),
    Symbolic(String),
}

/// Look up one full ref name: the loose file first (it wins over
/// `packed-refs`, same as git), then `packed-refs`. `None` if neither has it.
fn read_ref(repo: &Path, refname: &str) -> Result<Option<RefValue>> {
    // Refuse anything that could step outside the repo dir.
    if refname
        .split('/')
        .any(|c| c.is_empty() || c == "." || c == "..")
    {
        return Err(Error::BadRef {
            path: repo.join("HEAD"),
            detail: format!("odd ref name {refname:?}"),
        });
    }
    let loose = repo.join(refname);
    match fs::read_to_string(&loose) {
        Ok(s) => {
            let s = s.trim();
            return Ok(Some(match s.strip_prefix("ref:") {
                Some(t) => RefValue::Symbolic(t.trim().to_owned()),
                None => RefValue::Sha(parse_sha(s, &loose)?),
            }));
        }
        // NotADirectory/IsADirectory can happen with odd ref layouts; treat as
        // "not a loose ref" and fall through to packed-refs.
        Err(e) if e.kind() == io::ErrorKind::NotFound || loose.is_dir() => {}
        Err(e) => {
            return Err(Error::Io {
                path: loose,
                source: e,
            });
        }
    }

    let packed = repo.join("packed-refs");
    let text = match fs::read_to_string(&packed) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(Error::Io {
                path: packed,
                source: e,
            });
        }
    };
    for line in text.lines() {
        // Skip the `# pack-refs with: ...` header and `^peeled` lines.
        if line.starts_with('#') || line.starts_with('^') {
            continue;
        }
        if let Some((sha, name)) = line.split_once(' ')
            && name == refname
        {
            return Ok(Some(RefValue::Sha(parse_sha(sha, &packed)?)));
        }
    }
    Ok(None)
}

fn parse_sha(s: &str, path: &Path) -> Result<String> {
    if s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(s.to_ascii_lowercase())
    } else {
        Err(Error::BadRef {
            path: path.to_path_buf(),
            detail: format!("expected a sha, got {s:?}"),
        })
    }
}

// ---------------------------------------------------------------------------
// Trees and blobs, via libgit2
// ---------------------------------------------------------------------------

/// What a tree entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Blob,
    Tree,
    /// A gitlink (mode 160000). Its oid is a commit in another repo, so there
    /// is nothing to read here.
    Submodule,
}

/// One entry of a tree. `path` is relative to the repo root, `/`-separated,
/// with no leading slash. Non-UTF-8 names are converted lossily.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub oid: Oid,
    pub kind: EntryKind,
    /// The raw git file mode, e.g. `0o100644`, `0o100755`, `0o120000` (symlink).
    pub mode: i32,
    /// Byte size for blobs (read from the object header, not the content);
    /// `None` for trees and submodules.
    pub size: Option<u64>,
}

/// A read-only handle on one bare repo.
///
/// Not `Sync`: open one per repo per thread. Nothing it does writes to disk.
pub struct RepoReader {
    repo: Repository,
}

impl RepoReader {
    /// Open the bare repo at `path` exactly (no searching parent directories,
    /// no environment variables). `git_repository_open_bare` also skips
    /// libgit2's "safe.directory" ownership check, which matters because the
    /// hub runs as a different user than Forgejo.
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            repo: Repository::open_bare(path)?,
        })
    }

    /// Every entry under the commit's root tree, recursively, in pre-order
    /// (a directory comes before its contents). Includes the tree entries
    /// themselves; filter on `kind` if you only want files.
    pub fn tree(&self, commit_sha: &str) -> Result<Vec<Entry>> {
        let tree = self.commit(commit_sha)?.tree()?;
        let odb = self.repo.odb()?;
        let mut out = Vec::new();
        let mut failed = None;
        tree.walk(TreeWalkMode::PreOrder, |dir, e| {
            let name = String::from_utf8_lossy(e.name_bytes());
            let path = format!("{dir}{name}");
            match self.entry(&odb, path, e) {
                Ok(entry) => {
                    out.push(entry);
                    TreeWalkResult::Ok
                }
                Err(err) => {
                    failed = Some(err);
                    TreeWalkResult::Abort
                }
            }
        })
        .or_else(|e| if failed.is_some() { Ok(()) } else { Err(e) })?;
        match failed {
            Some(e) => Err(e),
            None => Ok(out),
        }
    }

    /// Only the top-level entries of the commit's tree (files and dirs),
    /// in git's tree order. Enough to find `manifest.yaml`, `manifest.json`,
    /// `metadata.json` and friends without walking the whole repo.
    pub fn root_files(&self, commit_sha: &str) -> Result<Vec<Entry>> {
        let tree = self.commit(commit_sha)?.tree()?;
        let odb = self.repo.odb()?;
        tree.iter()
            .map(|e| {
                self.entry(
                    &odb,
                    String::from_utf8_lossy(e.name_bytes()).into_owned(),
                    &e,
                )
            })
            .collect()
    }

    /// The full content of a blob.
    pub fn blob(&self, oid: Oid) -> Result<Vec<u8>> {
        Ok(self.repo.find_blob(oid)?.content().to_vec())
    }

    /// Content of the file at `path` (e.g. `"manifest.yaml"` or `"01/02.txt"`)
    /// in the given commit. `None` if there is no such path or it is not a
    /// blob (a directory or submodule).
    pub fn file(&self, commit_sha: &str, path: &str) -> Result<Option<Vec<u8>>> {
        let tree = self.commit(commit_sha)?.tree()?;
        let entry = match tree.get_path(Path::new(path)) {
            Ok(e) => e,
            Err(e) if e.code() == git2::ErrorCode::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if entry.kind() != Some(ObjectType::Blob) {
            return Ok(None);
        }
        self.blob(entry.id()).map(Some)
    }

    /// Committer time of the commit, as Unix seconds (UTC).
    pub fn commit_time(&self, commit_sha: &str) -> Result<i64> {
        Ok(self.commit(commit_sha)?.time().seconds())
    }

    fn commit(&self, sha: &str) -> Result<git2::Commit<'_>> {
        let oid = Oid::from_str(sha).map_err(|_| Error::BadOid(sha.to_owned()))?;
        Ok(self.repo.find_commit(oid)?)
    }

    fn entry(&self, odb: &git2::Odb<'_>, path: String, e: &git2::TreeEntry<'_>) -> Result<Entry> {
        let mode = e.filemode();
        let (kind, size) = match e.kind() {
            Some(ObjectType::Tree) => (EntryKind::Tree, None),
            Some(ObjectType::Commit) => (EntryKind::Submodule, None),
            // Blobs (incl. symlinks). The header read decodes just enough of
            // the object to learn its size; it does not inflate the content.
            _ => (EntryKind::Blob, Some(odb.read_header(e.id())?.0 as u64)),
        };
        Ok(Entry {
            path,
            oid: e.id(),
            kind,
            mode,
            size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway "bare repo" holding only ref files, enough for `head()`.
    fn fake_repo(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hub-repo-read-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for (p, content) in files {
            let p = dir.join(p);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, content).unwrap();
        }
        dir
    }

    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";

    #[test]
    fn head_loose_ref() {
        let r = fake_repo(
            "loose",
            &[
                ("HEAD", "ref: refs/heads/main\n"),
                ("refs/heads/main", &format!("{A}\n")),
            ],
        );
        assert_eq!(
            head(&r).unwrap(),
            Some(Head {
                branch: Some("main".into()),
                sha: A.into()
            })
        );
    }

    #[test]
    fn head_packed_ref_with_peeled_lines() {
        let packed = format!(
            "# pack-refs with: peeled fully-peeled sorted \n{B} refs/heads/master\n{A} refs/tags/v1\n^{B}\n"
        );
        let r = fake_repo(
            "packed",
            &[
                ("HEAD", "ref: refs/heads/master\n"),
                ("packed-refs", &packed),
            ],
        );
        assert_eq!(
            head(&r).unwrap(),
            Some(Head {
                branch: Some("master".into()),
                sha: B.into()
            })
        );
    }

    #[test]
    fn loose_ref_wins_over_packed() {
        let r = fake_repo(
            "both",
            &[
                ("HEAD", "ref: refs/heads/master\n"),
                ("refs/heads/master", A),
                ("packed-refs", &format!("{B} refs/heads/master\n")),
            ],
        );
        assert_eq!(head(&r).unwrap().unwrap().sha, A);
    }

    #[test]
    fn head_unborn_and_detached() {
        let r = fake_repo(
            "unborn",
            &[
                ("HEAD", "ref: refs/heads/main\n"),
                ("packed-refs", "# pack-refs\n"),
            ],
        );
        assert_eq!(head(&r).unwrap(), None);
        let r = fake_repo("detached", &[("HEAD", &format!("{A}\n"))]);
        assert_eq!(
            head(&r).unwrap(),
            Some(Head {
                branch: None,
                sha: A.into()
            })
        );
    }

    #[test]
    fn head_branch_with_slash() {
        let r = fake_repo(
            "slash",
            &[
                ("HEAD", "ref: refs/heads/bob/scribe\n"),
                ("refs/heads/bob/scribe", A),
            ],
        );
        assert_eq!(
            head(&r).unwrap().unwrap().branch.as_deref(),
            Some("bob/scribe")
        );
    }

    /// Smoke test against the real dev data, if it is there. Read-only.
    #[test]
    fn real_repos_if_present() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev-data/repositories");
        if !root.is_dir() {
            return;
        }
        let repos = list_repos(&root).unwrap();
        assert!(repos.len() > 100);
        // The one known-empty repo has an unborn HEAD.
        let empty = repos
            .iter()
            .find(|r| r.owner == "dan" && r.name == "aac_rom_text_reg")
            .unwrap();
        assert_eq!(head(&empty.path).unwrap(), None);
        // A normal repo: HEAD agrees with libgit2, and the tree is readable.
        let r = repos
            .iter()
            .find(|r| head(&r.path).unwrap().is_some())
            .unwrap();
        let h = head(&r.path).unwrap().unwrap();
        let reader = RepoReader::open(&r.path).unwrap();
        assert_eq!(
            reader.repo.head().unwrap().target().unwrap().to_string(),
            h.sha
        );
        let root_files = reader.root_files(&h.sha).unwrap();
        assert!(!root_files.is_empty());
        let all = reader.tree(&h.sha).unwrap();
        let first_blob = all.iter().find(|e| e.kind == EntryKind::Blob).unwrap();
        let bytes = reader.file(&h.sha, &first_blob.path).unwrap().unwrap();
        assert_eq!(bytes.len() as u64, first_blob.size.unwrap());
        assert_eq!(reader.file(&h.sha, "no/such/file").unwrap(), None);
        assert!(reader.commit_time(&h.sha).unwrap() > 1_000_000_000);
    }
}
