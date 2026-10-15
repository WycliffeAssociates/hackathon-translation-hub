# Product

## Register

product

## Users

**Publishers and language leads** at Wycliffe Associates. They decide which translated books become a language's official text, pull updates into it, and cut releases with bulletins (release notes). They need to see, book by book, which copies exist on WACS, how they differ, and which one to trust.

**WA admins and managers** oversee WACS as a whole: duplicate and abandoned repos, books shown twice on BIEL, what is alive, and progress across languages against the gateway sources.

Both work at a desk, in long sessions, making decisions that other people will read and rely on. Translators and field partners are not the primary audience of these screens.

## Product Purpose

The Translation Hub sits beside WACS (Forgejo). It reads every repo, proofreads the books, compares them with their sources, and lets publishers turn scattered Writer projects into one official, released text per language and brand. It exists because tStudio and Writer cannot be retired, so the same book lives in many repos and nobody can see which is current.

Success: a publisher opens a language, sees for each book what the options are and how they differ, picks with confidence, and publishes a release whose bulletin says what changed. An admin can find what needs attention without reading every repo.

## Brand Personality

Quiet, exact, trustworthy. Like a well-kept ledger: little chrome, the text and the numbers carry the page. Calm enough to work in all day; precise enough that a number on screen can be acted on. The voice is plain and specific ("3 copies of Titus; 2 are identical"), never chatty or celebratory.

References:
- **Joe's hub** (Bible-Translation-Tools/translation-hub): plain pages, one job per screen, a personal home ("your languages, your projects"), titles that name the language and the resource.
- **GitHub repo pages**: tabs for Files, Releases and History; a file tree; facts in a quiet side column.
- **Sefer** (our editor): the hub and the editor should feel like one product.

## Anti-references

- **Chip and badge soup**: tags, pills and counts on every row. The current hub has drifted into this; Joe's version is the correction.
- **SaaS analytics dashboard**: big-number tiles, KPI cards, gradients.
- **The vibe-coded AI look**: cream or paper backgrounds, serif headings everywhere, small uppercase eyebrow labels, cards inside cards.
- **Enterprise admin**: grey forms, dense toolbars, modal-heavy workflows.

## Design Principles

1. **The text is the evidence.** A finding, a comparison or a progress number always leads to the verses behind it, one click away.
2. **Decide by book.** Repos are an accident of tStudio and Writer; the publisher's real unit is the book. Group, compare and choose at that level.
3. **Say less, mark only what matters.** One signal per row at most; status that is normal is not shown.
4. **For you first, everything one search away.** A home tailored to the person's languages and projects, with search always present.
5. **Writes are deliberate.** Anything that changes WACS (adopt, pull, publish) says exactly what it will do before it does it, and records who did it.

## Accessibility & Inclusion

No formal target for the hackathon proof of concept. Sensible defaults still apply: readable contrast, keyboard access to every action, reduced motion respected, and colour never the only signal (severities, chart series).
