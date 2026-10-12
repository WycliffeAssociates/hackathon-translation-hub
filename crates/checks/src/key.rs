//! Content keys: a stable name for "these exact bytes".
//!
//! A lint summary depends only on the book's bytes and the engine version, so
//! it is cached by `(content_key, KITCHEN_TAG)`. For an RC repo the book is one
//! git blob, and the blob SHA already names its bytes. For a tS (Writer) repo
//! the book is stitched from many chunk blobs, so the key hashes the sorted
//! `(path, blob_sha)` list: any chunk edited, added, removed or renamed changes it.

use xxhash_rust::xxh3::xxh3_64;

/// The content key of a book stored as a single blob: the blob SHA itself.
pub fn content_key_for_blob(blob_sha: &str) -> String {
    blob_sha.to_owned()
}

/// The content key of a book stitched from chunks: hex xxh3-64 over the
/// `path\0sha\n` lines, sorted, so the order the caller listed them in does
/// not matter.
pub fn content_key_for_chunks<P: AsRef<str>, S: AsRef<str>>(chunks: &[(P, S)]) -> String {
    let mut lines: Vec<(&str, &str)> = chunks
        .iter()
        .map(|(path, sha)| (path.as_ref(), sha.as_ref()))
        .collect();
    lines.sort_unstable();
    let mut buf = Vec::new();
    for (path, sha) in lines {
        buf.extend_from_slice(path.as_bytes());
        buf.push(0);
        buf.extend_from_slice(sha.as_bytes());
        buf.push(b'\n');
    }
    format!("{:016x}", xxh3_64(&buf))
}

/// The lint cache key: content key plus the engine tag it was linted with.
pub fn cache_key(content_key: &str) -> String {
    format!("{content_key}@{}", crate::KITCHEN_TAG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_key_is_the_sha() {
        assert_eq!(content_key_for_blob("abc123"), "abc123");
        assert_eq!(cache_key("abc123"), format!("abc123@{}", crate::KITCHEN_TAG));
    }

    #[test]
    fn chunk_key_ignores_order_but_not_content() {
        let a = content_key_for_chunks(&[("01/01.txt", "aa"), ("01/02.txt", "bb")]);
        let b = content_key_for_chunks(&[("01/02.txt", "bb"), ("01/01.txt", "aa")]);
        let c = content_key_for_chunks(&[("01/01.txt", "aa"), ("01/02.txt", "bc")]);
        let d = content_key_for_chunks(&[("01/01.txt", "aa"), ("01/03.txt", "bb")]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert_eq!(a.len(), 16);
    }
}
