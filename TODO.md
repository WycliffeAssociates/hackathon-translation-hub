# TODO: upstream fixes (scripture-kitchen, Sefer)

Things the hub found that belong in other repos. Will fixes these there; the
hub only pins the result. Numbers are from dev-data (1,441 repos) on
2026-10-12; `/checks` in the hub shows the current ones.

## scripture-kitchen

- [ ] **Sous `MisplacedFormat` should judge format characters against the corpus.** It is 971k of 1.05M Sous findings, and nearly all of them come from writing systems that use these characters on purpose:

  | Repo | Character | Findings |
  |---|---|---|
  | `wa-catalog/km_ulb` | U+200B ZERO WIDTH SPACE (Khmer word breaks) | 300k |
  | `wa-catalog/lo_ulb` | U+200B (Lao) | 195k |
  | `wa-catalog/fa_opv` | U+200C ZWNJ, word-final (Persian) | 235k |
  | `wa-catalog/hbo_uhb` | U+2060 WORD JOINER (Hebrew) | 163k |
  | `wa-catalog/th_ulb` | U+200B (Thai) | 3k |

  The convention rules already learn what a project normally does. A format character that occurs thousands of times in a corpus is that corpus's convention, not a hygiene slip. One verse also yields many rows (one per character run), so the counts overstate the sites.
- [ ] **Sous `Rarity` is noisy on one-book corpora.** A Writer book is a corpus of one (e.g. gl_rom_text_reg: 21 `Rarity` findings, all capital letters, "[C] appears only 3 times"). Consider an abstention threshold on corpus size.
- [ ] **`\s5` in Sous.** `galley::sous::OnionBook::parse` uses the default lexer, so the hub cannot apply Sefer's `\s5` policy there as `hub-checks` does for lint (`lex_with` + a registry). An `OnionBook::parse_with(text, &Extensions)` would let it.
- [ ] **Lint `delimiter-surplus`** (34,964 info findings in 414 repos; 39% in one repo) is formatter-level. Check whether its severity or category is right. The hub counts `hint` and `info` together and skips only severity `form`.
- [ ] **Sous book keys**: `\id` is taken as written (first three bytes), so `\id gen` and `\id GEN` are two books. Upper-case it, as onion's canon does.
- [ ] **Push tag v0.1.10** (it exists only in the local clone), then bump every pin together: root `Cargo.toml` (`usfm_onion`, `mise`), `crates/proof/Cargo.toml` (`usfm_galley`, `sous-core`), `spikes/viewer/package.json`, and `KITCHEN_TAG` in `crates/checks` and `crates/proof`. Bumping makes the hub rescan and re-proofread everything on its next poll.

## Sefer

- [ ] **The Writer stitch template opens no paragraph.** Writer never writes `\p`, and `writerBookToUsfm` adds none. So lint `missing-paragraph` fires once per chapter in 952 of 983 synthetic books (9,819 warnings), and migrated repos inherit it. Emitting `\p` after each `\c` (and `\cl`) in `src/core/migrate/writerBook.ts` fixes both. Then re-port `crates/stitch` (it is byte-identical to Sefer 50aa9cd on 987 repos) and re-run its golden.
