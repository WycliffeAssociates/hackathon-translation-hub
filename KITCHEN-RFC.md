# Kitchen RFC: `OnionBook::locate` walks the whole mask for every call

**Where:** scripture-kitchen `v0.1.9`, `galley/src/sous/onion_book.rs`,
`OnionBook::locate` and `SourceSpans` (checked against the git checkout the hub
builds from; the hub pins the tag and does not edit the kitchen).

**Found:** 2026-10-14, profiling the hub's proofreading of `wa-catalog/hbo_uhb`
(39 books, 28 MB, 163,284 findings) with samply.

## What happens

```rust
pub fn locate(&self, range: TextRange) -> Option<LocatedRange<'_>> {
    // first, last: two TOC lookups (fine)
    Some(LocatedRange { first, last, spans: SourceSpans { mask: &self.mask, projected: range, at: 0 } })
}

impl Iterator for SourceSpans<'_> {
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(source) = self.mask.ranges.get(self.at) { ...; self.at += 1; if overlaps { return Some(..) } }
        None
    }
}
```

1. **It starts at range 0** (`at: 0`) instead of the first mask range that can
   overlap `range`.
2. **It never stops early.** After the last overlapping range, `next()` keeps
   scanning to the end of `mask.ranges` before it returns `None`. Any caller that
   drains the iterator (`.last()`, `.collect()`, a `for` loop) pays for the whole
   mask.

So one `locate` plus a drain costs O(mask ranges), and a caller that locates
every finding pays O(findings × mask ranges).

## What it cost

The hub maps each Sous finding to raw bytes for the editor
(`hub_proof::Target::raw`): `locate(span)`, then the first run's start and the
last run's end. On hbo_uhb that was **90% of a 4 s proofread**. The analysis
itself (sous-core) was about 0.2 s. km_ulb (301k findings) and fa_opv (236k) are
the same shape.

The hub now keeps its own copy of the mask (it builds the book with
`OnionBook::from_parts` from the same lex/cst/toc/mask that `parse` uses) and
maps with `Mask::to_source`, which already binary-searches. The output is
byte-identical on en_ulb, hbo_uhb, km_ulb and ru_ulb with its source.
hbo_uhb now proofreads in 237 ms.

## Proposal

Any one of these would fix it for every caller (Sefer, the hub, the CLI):

1. **Start where the range starts, stop where it ends** (the smallest change):

   ```rust
   let at = self.mask.starts.partition_point(|&s| s <= range.from()).saturating_sub(1);
   SourceSpans { mask: &self.mask, projected: range, at }
   // in next(): once `projected_start >= self.projected.to()`, return None
   ```

   Cost becomes O(log ranges + runs in the range).

2. **Expose the bounding raw span directly**, the question most callers ask:

   ```rust
   /// The raw bytes from the first projected byte of `range` to one past its last.
   pub fn raw_bounds(&self, range: TextRange) -> Option<Range<u32>> {
       (!range.is_empty()).then(|| self.mask.to_source(range.from())..self.mask.to_source(range.to() - 1) + 1)
   }
   ```

3. **Expose the mask** (`pub fn mask(&self) -> &Mask`), so a host does not have
   to rebuild it beside the book as the hub does now.

(1) is a pure fix. (2) and (3) are small additions. The hub would drop its copy
of the mask (`parse_book` in `crates/proof/src/lib.rs`) after any of them.

## Check

A test with a book of many short verses (thousands of mask ranges) that locates
a range near the end and drains `spans`, asserting the same runs as today and
that `next()` touches only the ranges it returns plus one. Or time it:
`cargo run --release -p hub-proof --example proofread_repo -- <hbo_uhb.git>` in
the hub, before and after a bump.
