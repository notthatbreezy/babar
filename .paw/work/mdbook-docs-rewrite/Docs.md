# The Book of Babar — Documentation Site Rewrite

## Overview

This work rewrites babar's mdbook documentation site as *The Book of
Babar*: a Diataxis-aligned, conversational, code-first reference that
takes its voice from typelevel doobie's *Book of Doobie*. No API or
crate code changes — this is a documentation-site-only deliverable.

The site lives under `docs/` (mdbook `src=docs`), builds with `mdbook
build` to `book/` (gitignored), and publishes to the project's CNAME.
The verbatim `docs/tutorials/postgres-api-from-scratch.md` is
preserved byte-for-byte; everything else is new or rewritten.

## Architecture and Design

### Information architecture (Diataxis)

The site is organized into four Diataxis quadrants, surfaced in
`docs/SUMMARY.md`:

- **Get Started** — Prerequisites, first query (action + acquisition).
- **Book** — Thirteen numbered chapters from Connecting through
  Observability (action + acquisition; the doobie analog).
- **Reference** — Codecs, errors, feature flags, configuration
  (cognition + application).
- **Explanation** — Why babar, What makes babar babar, Design
  principles, Driver task, Comparisons, Roadmap (cognition +
  acquisition).
- **Tutorials** — preserved verbatim (the original Postgres-API-from-
  scratch walkthrough).

The landing page (`docs/index.md`) routes new readers into Get Started
and offers "Where to go next" links — *What makes babar babar* first,
then chapters by intent.

### Voice conventions

Voice target is typelevel doobie's *Book of Doobie*. Conventions:

- **Second person, conversational.** "In this chapter we'll…", "Let's
  break this down", "you'll meet…".
- **Code first.** Each section opens with a runnable Rust block, then
  prose explains what just happened. Avoid prose-heavy preambles.
- **Inline type annotations.** Rust expressions whose Rust type isn't
  obvious from the line carry a `// type: T` comment, e.g.
  `let session: Session = Session::connect(...)  // type: Session`.
- **American English.** `behaviour|colour|organis|optimis|catalogu`
  are forbidden. "catalog", not "catalogue".
- **One idea per paragraph.** Short paragraphs, hard wrap at ~72 cols.
- **No marketing.** Trade-offs and limitations get equal billing with
  capabilities. The Comparisons page and the "What babar deliberately
  does not do" sections both read as honest catalogs.

### Asset locations

- Images live at `docs/assets/img/` and are referenced with paths
  relative to the markdown file (`./assets/img/foo.png` from
  `docs/index.md`, `../assets/img/foo.png` from a subdir).
- The legacy `docs/landing-mockup.html` and `docs/SITE-COPY.md`
  authoring artifacts were relocated to `.design/` in Phase 1.
- The root-owned `images/` directory at the repo root is pre-existing
  legacy and not part of the site.

### Cross-cutting design decisions

1. **Strict separation between Book and Reference.** The Book teaches;
   the Reference catalogs. Reference pages link to authoritative
   rustdoc rather than reproducing it.
2. **Explanation pages own the philosophy.** *What makes babar babar*
   is the canonical "if you only read one page" tour; *Why babar* is
   the elevator pitch; *Design principles* is the rule book; *Driver
   task* is the deep dive.
3. **No inline DSN parsing.** Examples consistently use `Config::new`
   with positional arguments and chained methods, never a DSN string.
   This is a Spec FR-002 hard requirement.
4. **Prerequisites is canonical.** A single foreground `docker run`
   command for `postgres:17` with verbose query logging is the only
   blessed local-dev setup. All chapter examples assume the connection
   string `postgres://babar:babar@localhost:5432/babar`.
5. **Comparisons page self-flags.** The third-party-driver claims are
   shipped as-is with an explicit invitation to file an issue if a
   claim drifts. Named comparisons live only in `comparisons.md`;
   other explanation pages use soft phrasing ("some other Postgres
   drivers…").

## User Guide

### Building the site locally

```sh
rm -rf book && mdbook build
```

Output: `book/`. Open `book/index.html` in a browser. The build is
warning-free; any warning is a regression.

For watch mode during authoring:

```sh
mdbook serve --port 3000
```

### Adding a new page

1. Create the markdown file under the appropriate Diataxis quadrant
   (`docs/getting-started/`, `docs/book/`, `docs/reference/`,
   `docs/explanation/`).
2. Add an entry to `docs/SUMMARY.md` in hierarchy order.
3. Open with the convention for that quadrant:
   - Book chapter: `# N. Title` then `In this chapter we'll…` then a
     `## Setup` code block.
   - Reference: `# Title` then a one-line generated-rustdoc pointer
     (`> Generated rustdoc: <https://docs.rs/...>`).
   - Explanation: `# Title` then `> See also: ...` then prose.
4. Add cross-links to/from related pages.
5. Run the gates (see Testing).

### Updating an existing page

Read the surrounding chapters first to keep voice consistent. Run the
chapter-shape and Am-En gates after editing.

## Testing

### Mandatory gates (run before commit)

```sh
# Build is clean
rm -rf book && mdbook build

# American English
grep -RInE '(behaviour|colour|organis|optimis|catalogu)' docs/ \
  | grep -v 'tutorials/postgres-api-from-scratch.md'
# expect: no output

# Common typos (double words, missing-space-after-period)
grep -RIn -E '\bin in\b|\bthe the\b|\b[a-z]+\.[A-Z][a-z]' docs/ \
  | grep -v 'tutorials/postgres-api' | grep -v 'docs.rs/babar'
# expect: no output

# Tutorial preserved verbatim
git diff main -- docs/tutorials/postgres-api-from-scratch.md
# expect: empty

# No crate changes
git diff --stat main -- crates/ '**/Cargo.toml'
# expect: empty

# No Zone.Identifier files in git tracking
! git ls-files | grep -i 'zone.identifier'
```

### Chapter-shape gate (heuristic)

```sh
for f in docs/book/*.md; do
  head -80 "$f" | grep -qE '\byou(r)?\b' || echo "NO 2nd-person: $f"
  awk 'BEGIN{infence=0;sawcode=0} /^```/{infence=!infence; if(infence) sawcode=1; next} !infence && /^[A-Za-z]/ && sawcode==0 {print FILENAME": prose-before-code"; exit} ' "$f"
done
```

This is coarse on purpose. "prose-before-code" output is normal for
chapters that open with a one-paragraph "In this chapter we'll…"
sentence, matching the doobie sample. "NO 2nd-person" output for a
chapter whose first 80 lines are mostly setup code is acceptable when
the chapter clearly uses second-person elsewhere.

### Manual verification

- Click through every SUMMARY entry in the built site.
- Read landing → Get Started → three random Book chapters → one
  Reference page → one Explanation page in a single sitting and
  confirm voice consistency.

## Limitations and Future Work

- **Comparisons page** ships with ten third-party driver claims
  surfaced for sign-off but not independently verified against
  upstream. The page self-flags this and invites issues.
- **Filesystem Zone.Identifier scan** flags pre-existing root-owned
  files in the repo's untracked `images/` directory. The
  git-tracking gate (which is what actually matters for shipping) is
  clean.
- **Reference rustdoc links** point at `docs.rs/babar/latest/` and
  will resolve once a release is published; until then they 404.
- **Search.** mdbook's built-in search is enabled by default; no
  custom indexing or analytics are wired up.
- **Internationalization.** The site is American English only.
