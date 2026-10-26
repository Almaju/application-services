# What our reviews keep repeating, and which of it a linter could catch

_Research snapshot, 2026-10-26, `main` @ 5335439. Built from public PR data for mozilla/application-services. The
method, scripts and raw results are in this directory: [README.md](README.md)._

## TL;DR

- This covers **every PR review comment in the repo**: 13,862 inline comments, 3,982 review summaries and 6,514 conversation
  comments across 5,051 PRs (2018 → today). The 4,333 reviewer threads since 2022 were read in full with LLM help and
  turned into **485 candidate rules**, which merge into **[91 distinct rules](RULES.md)**.
- **Most of what we repeat in review is mechanical.** 75 of the 91 rules (82%) need only configuration or a small
  script. They cover 1,560 of the 1,869 review threads behind the rules. Only 9 rules really have to stay as human
  judgement.
- **It also costs us bugs.** Of 204 real bugfix PRs since 2022, **52 (25%) would have been prevented by an automated
  rule** and 51 more partially. The other half are genuine logic/product bugs no linter will see. (The count includes
  ~10 backports of the same fix.)
- **Our PR template already asks** about breaking changes, tests, CHANGELOG and dependencies, and those are still among
  the most repeated review comments. The checkboxes haven't stopped them; automated checks could.
- **UniFFI is the biggest single theme.** The #1 finding overall is a binding API change landing without a "⚠️ Breaking
  Changes" note: 62 threads in ≥41 PRs, in every period since 2022. Swift name collisions, panics in lifts, missing
  docstrings and Record defaults follow ([UniFFI section](#uniffi-footguns)).
- **Cheapest first step:** a `[workspace.lints]` table plus a real `clippy.toml`
  ([prototype/workspace-lints.toml](prototype/workspace-lints.toml), [prototype/clippy.toml](prototype/clippy.toml)).
  Twelve of the proposed lints have **zero** violations today and could be `deny` tomorrow.
- **Measuring turned up some live issues** ([below](#things-found-while-measuring)), e.g. ads-client cache indexes that
  older installs never get, and relay logging through the `log` crate (the bug #7332 fixed elsewhere).

## Data and method

| Source | Count | Used for |
|---|---:|---|
| PRs (all states) | 5,051 | titles/bodies, bugfix selection |
| Inline review comments | 13,862 | reviewer threads (root comment + replies + diff hunk) |
| Review summaries with text | 3,982 | reviewer threads |
| PR conversation comments | 6,514 | (mostly bots/CI; not used for rules) |
| Reviewer threads after filtering | 9,030 | keyword stats, all years ([results/tag_stats.md](results/tag_stats.md)) |
| …of which since 2022 | 4,333 | read in full, 7 chunks by date |
| Likely bugfix PRs since 2022 (title keywords) | 291 | diff read in full → 204 real bugfixes |
| Merged PRs since 2025 | 1,028 | backtesting the diff-based gates |

1. **Fetch** everything via the REST API ([scripts/fetch.py](scripts/fetch.py)). Drop bots, "thanks/done/+1"-style
   acks, and authors annotating their own PR ([scripts/prepare.py](scripts/prepare.py)).
2. **Extract.** Seven subagents each read one ~115k-token chunk (2022 → 2026) and wrote down every recurring mistake as
   a candidate rule, with counts and verbatim evidence ([prompt](prompts/extract_rules.md)). Two more read the 291
   bugfix-PR diffs and judged "would a rule have prevented this?" ([prompt](prompts/assess_bugfixes.md)).
3. **Merge** the 485 candidates into 91 rules by hand ([scripts/canon.py](scripts/canon.py)). Rank them by distinct PRs,
   persistence (how many of the 7 periods they show up in) and bugfix PRs.
4. **Measure** each rule against today's code where possible: clippy with candidate lints over the whole workspace
   (`--workspace --lib`, [results/clippy_counts.md](results/clippy_counts.md)), ast-grep rules and small Python checks
   ([prototype/](prototype/)), and a backtest of the diff-based gates on 1,028 recent PRs.

**Caveats.** Counts come from LLM reading, so treat them as approximate. The evidence is solid: 2,054 of the 2,056
quotes match their source comment exactly ([scripts/verify_quotes.py](scripts/verify_quotes.py); the other two are a
lone "?" and an emoji). The counts themselves were not re-checked. "Threads" can over-count: one PR (#7367) produced
most of the 13 threads behind one rule, which is why the tables also show distinct PRs (a lower bound from the sampled
evidence) and periods. Comments are quoted without naming anyone; this is about our process, not about people.

## What keeps coming up

Ranked by distinct PRs + persistence + bugfixes. All 91 are in [RULES.md](RULES.md), each with review quotes, linked
bugfixes and today's hit count.

| # | Rule | Area | Threads (≥ PRs) | Periods | Bugfix PRs | Tier |
|---:|---|---|---:|---:|---:|---|
| 1 | [Detect binding API changes and require a ⚠️ Breaking Changes entry](RULES.md#uniffi-api-break-gate) | UniFFI | 62 (≥41) | 7/7 | 2 | 2 |
| 2 | [No `unwrap()`/`expect()`/`panic!` in non-test component code](RULES.md#no-unwrap-in-component-code) | Errors | 46 (≥23) | 6/7 | 8 | 0 |
| 3 | [Spell-check identifiers, comments, docs, UDL and CHANGELOG](RULES.md#spellcheck) | Docs | 73 (≥38) | 7/7 | | 0 |
| 4 | [Use Option/Result combinators and `let-else` instead of manual matches](RULES.md#option-result-combinators) | Idioms | 60 (≥34) | 6/7 | | 0 |
| 5 | [Every source file starts with the MPL-2.0 header and ends with a newline](RULES.md#license-header-and-whitespace) | Docs | 28 (≥22) | 5/7 | 6 | 1 |
| 6 | [Avoid needless clones and allocations; write `Arc::clone(&x)`](RULES.md#needless-clones) | Idioms | 54 (≥32) | 7/7 | | 0 |
| 7 | [Any schema change bumps `VERSION`, adds an `upgrade_from` step and an upgrade test](RULES.md#schema-change-needs-migration) | SQL | 39 (≥25) | 7/7 | 3 | 1 |
| 8 | [Doc comments reference things that exist (intra-doc links, stale names)](RULES.md#docs-in-sync) | Docs | 56 (≥32) | 6/7 | | 0 |
| 9 | [Descriptive identifiers: no cryptic abbreviations, follow Rust API naming](RULES.md#naming) | Naming | 50 (≥32) | 6/7 | | 1 |
| 10 | [Keep public error enums small and actionable; one reported catch-all with a reason](RULES.md#uniffi-public-errors-actionable) | UniFFI | 39 (≥28) | 7/7 | 1 | 3 |
| 11 | [A small curated pedantic subset (derives, unit structs, `Default` use)](RULES.md#pedantic-subset) | Idioms | 52 (≥31) | 6/7 | | 0 |
| 12 | [Every exported item has a `///` docstring](RULES.md#uniffi-docstrings) | UniFFI | 50 (≥29) | 6/7 | | 1 |
| 13 | [New/bumped crates must be vetted and compatible with mozilla-central](RULES.md#dependency-vetting) | Deps | 25 (≥22) | 6/7 | 3 | 1 |
| 14 | [CHANGELOG edits go in the in-progress section, under a component heading, with a PR link](RULES.md#changelog-format) | Docs | 36 (≥26) | 6/7 | | 1 |
| 15 | [Let rustfmt own imports and layout](RULES.md#rustfmt-config) | Idioms | 44 (≥26) | 6/7 | | 0 |
| 16 | [Every PR compiles all targets a change can break (wrappers, examples, tests, taskgraph)](RULES.md#compile-everything-on-pr) | Build | 9 (≥8) | 4/7 | 10 | 1 |
| 17 | [New/renamed components are registered everywhere (megazords, uniffi.toml, Swift, Gradle, CI)](RULES.md#component-wiring) | Build | 25 (≥14) | 5/7 | 5 | 1 |
| 18 | [Name and explain magic values and non-obvious intent](RULES.md#explain-magic-values) | Docs | 34 (≥22) | 6/7 | | 3 |
| 19 | [Tests are isolated: no wall clock, shared global state, fixed DB paths or prod endpoints](RULES.md#test-isolation) | Tests | 8 (≥7) | 3/7 | 9 | 1 |
| 20 | [Use the narrowest visibility (`pub(crate)`/private)](RULES.md#minimal-visibility) | API | 31 (≥21) | 6/7 | | 0 |
| 21 | [New behaviour comes with tests in the same PR](RULES.md#tests-with-behaviour) | Tests | 46 (≥23) | 4/7 | | 1 |
| 22 | [Don't silently discard errors (`let _ =`, `.ok()`, `if let Ok`, `_ => None`)](RULES.md#dont-swallow-errors) | Errors | 36 (≥21) | 5/7 | | 0 |
| 23 | [Remove unused pub items, error variants and columns rustc can't see](RULES.md#remove-dead-pub-code) | Dead code | 25 (≥20) | 6/7 | | 1 |
| 24 | [New/optional Record fields get a UniFFI default matching Rust's `Default`](RULES.md#uniffi-record-defaults) | UniFFI | 24 (≥19) | 4/7 | 1 | 2 |
| 25 | [Types deserialized from server/Remote Settings data tolerate absent and unknown values](RULES.md#tolerant-remote-deserialization) | Remote data | 25 (≥18) | 4/7 | 1 | 1 |

**Tiers:** 0 = turn on an existing lint or setting, 1 = small script / ast-grep rule / CI diff check, 2 = custom
tooling, 3 = stays a review checklist item.

| Tier | Rules | Review threads covered |
|---|---:|---:|
| 0 · config only | 29 | 727 |
| 1 · small script / ast-grep | 46 | 833 |
| 2 · custom tooling | 7 | 139 |
| 3 · review checklist | 9 | 170 |

**Trends** (keyword tagging over all 9,030 threads, [results/tag_stats.md](results/tag_stats.md)): comments about
performance rose from 4% of threads (2023) to 13% (2025), tests from 9% (2024) to 16% (2026), and error handling from
10% (2024) to 15% (2026). Docs/comments stay around 20% every year.

## UniFFI footguns

| Rule | Threads (≥ PRs) | Periods | Bugfix PRs | Tier |
|---|---:|---:|---:|---|
| [Detect binding API changes and require a ⚠️ Breaking Changes entry](RULES.md#uniffi-api-break-gate) | 62 (≥41) | 7/7 | 2 | 2 |
| [Keep public error enums small and actionable; one reported catch-all with a reason](RULES.md#uniffi-public-errors-actionable) | 39 (≥28) | 7/7 | 1 | 3 |
| [Every exported item has a `///` docstring (it is what Kotlin/Swift consumers see)](RULES.md#uniffi-docstrings) | 50 (≥29) | 6/7 | | 1 |
| [Optional/new Record fields get a UniFFI default that matches Rust's `Default`; avoid `Option<Vec<_>>`](RULES.md#uniffi-record-defaults) | 24 (≥19) | 4/7 | 1 | 2 |
| [Exported type names must be unique across the megazord (Swift has one flat module)](RULES.md#uniffi-unique-exported-names) | 20 (≥12) | 4/7 | 2 | 1 |
| [Expose typed values over FFI (enums, Url, timestamps), not strings, magic ints or raw JSON](RULES.md#uniffi-typed-values) | 23 (≥16) | 4/7 | | 3 |
| [Exported names must read well in Kotlin/Swift and say what they do](RULES.md#uniffi-names-read-well) | 16 (≥14) | 3/7 | | 3 |
| [Never panic in lifts, custom-type conversions or exported constructors](RULES.md#uniffi-no-panic-across-ffi) | 5 (≥4) | 2/7 | 5 | 1 |
| [Keep the FFI layer thin: `Arc`, builders and serde stay at the boundary](RULES.md#uniffi-thin-ffi-layer) | 14 (≥9) | 4/7 | | 3 |
| [Exported fallible functions return `ApiResult` via `#[handle_error]`](RULES.md#uniffi-handle-error) | 13 (≥8) | 4/7 | | 1 |
| [Callback interfaces return `Result`, are released on shutdown, and user-waiting calls are async](RULES.md#uniffi-callback-contract) | 6 (≥3) | 1/7 | 2 | 2 |
| [Don't export test-only helpers over FFI](RULES.md#uniffi-no-test-only-exports) | 7 (≥5) | 3/7 | | 1 |
| [UDL and proc-macro declarations agree; new code uses proc-macros](RULES.md#uniffi-udl-proc-macro-consistency) | 12 (≥5) | 2/7 | | 1 |

What stands out:

- **Breaking changes are the top theme overall.** Reviewers catch them by eye ("technically it's a breaking change for
  our consumers"). When they miss one, a consumer finds it: a required `tabsLocalId` param broke firefox-ios (#4975),
  and new `@Deprecated` annotations broke Fenix's `-Werror` build (#6927). A grep can't gate this: a naive "removed
  export line without a Breaking note" check flags 136 of 156 API-touching PRs since 2025. **Proposal:** check in a
  per-component API snapshot generated from UniFFI metadata (library mode), the same idea as Kotlin's
  binary-compatibility-validator `.api` files. The diff then shows up in the PR, and CI requires a
  `⚠️ Breaking Changes ⚠️` entry when the snapshot loses or changes something.
- **One flat Swift module.** Two components exporting the same name breaks the iOS build: `Guid` in tabs vs places
  (#5181), `create_key` in autofill vs logins (#5293). Today there are **0 duplicates** among the 303 names exported by
  the 24 iOS-megazord crates, so a duplicate check ([prototype/checks.py](prototype/checks.py)) costs nothing to turn on.
  It also lists generic names likely to collide next (`ApiError`, `Headers`, `Backend`, `Level`, …).
- **Panics across the FFI.** webext-storage's `JsonValue` `try_lift` called `.unwrap()` on JSON from the caller and
  crashed. The fix landed four times (#6967 plus three release backports). #7578 was an `expect()` on server-echoed
  headers. Today `filter_adult`'s exported constructor returns `ApiResult` and still calls `.unwrap()` inside
  (`components/filter_adult/src/lib.rs:25`).
- **Docstrings are the binding docs.** A `//` line inside a `///` block silently dropped part of an exported fn's docs
  (#6462). Today 398 UDL declarations/methods and 87 proc-macro exported items have no `///`.
- **Wrappers aren't compiled on every PR.** A renamed error field broke `Tabs.swift` (#5223). Swift warnings leaked into
  consumer builds (#5259, #5272). A Swift wrapper recursed into itself forever (#5536). A Kotlin compile error (a missing
  `return`) landed because PR CI only builds the Kotlin module with `[ci full]` in the title (#7437).

## Bugfixes a rule would have prevented

Of the 291 PRs whose titles looked like fixes (2022 →), 204 were real bugfixes. The agents' verdicts: **52 yes**, **51
partially**, **101 no**. Spot-checks against the diffs: #6189 (two SQL statements passed to `execute()`, so
the second never ran), #5269 (a transaction never committed), #6967 (`unwrap` in a lift) and #5411 (byte-index
`truncate` panicking on multi-byte titles). All four match the verdicts.

| Rule | Prevented / partially |
|---|---|
| [No `unwrap()`/`expect()`/`panic!` in non-test component code](RULES.md#no-unwrap-in-component-code) | [#6967](https://github.com/mozilla/application-services/pull/6967), [#6971](https://github.com/mozilla/application-services/pull/6971), [#6972](https://github.com/mozilla/application-services/pull/6972), [#6973](https://github.com/mozilla/application-services/pull/6973), [#7273](https://github.com/mozilla/application-services/pull/7273), [#7536](https://github.com/mozilla/application-services/pull/7536), [#7578](https://github.com/mozilla/application-services/pull/7578) / [#6143](https://github.com/mozilla/application-services/pull/6143) |
| [Every PR compiles all targets that a change can break](RULES.md#compile-everything-on-pr) | [#5223](https://github.com/mozilla/application-services/pull/5223), [#5259](https://github.com/mozilla/application-services/pull/5259), [#5272](https://github.com/mozilla/application-services/pull/5272), [#5536](https://github.com/mozilla/application-services/pull/5536), [#7437](https://github.com/mozilla/application-services/pull/7437) / [#4920](https://github.com/mozilla/application-services/pull/4920), [#5610](https://github.com/mozilla/application-services/pull/5610), [#5689](https://github.com/mozilla/application-services/pull/5689), [#5695](https://github.com/mozilla/application-services/pull/5695), [#7226](https://github.com/mozilla/application-services/pull/7226) |
| [MPL header + whitespace (mozilla-central's linters)](RULES.md#license-header-and-whitespace) | [#6243](https://github.com/mozilla/application-services/pull/6243), [#6514](https://github.com/mozilla/application-services/pull/6514), [#6515](https://github.com/mozilla/application-services/pull/6515), [#6623](https://github.com/mozilla/application-services/pull/6623), [#6626](https://github.com/mozilla/application-services/pull/6626), [#6903](https://github.com/mozilla/application-services/pull/6903) / – |
| [Tests are isolated (clock, global state, NSS init)](RULES.md#test-isolation) | – / [#5385](https://github.com/mozilla/application-services/pull/5385), [#5403](https://github.com/mozilla/application-services/pull/5403), [#5449](https://github.com/mozilla/application-services/pull/5449), [#5785](https://github.com/mozilla/application-services/pull/5785), [#6032](https://github.com/mozilla/application-services/pull/6032), [#6033](https://github.com/mozilla/application-services/pull/6033), [#6126](https://github.com/mozilla/application-services/pull/6126), [#6728](https://github.com/mozilla/application-services/pull/6728), [#6730](https://github.com/mozilla/application-services/pull/6730) |
| [Shell scripts: strict mode + portability](RULES.md#shell-strict-shellcheck) | [#6392](https://github.com/mozilla/application-services/pull/6392), [#6512](https://github.com/mozilla/application-services/pull/6512), [#6849](https://github.com/mozilla/application-services/pull/6849) / [#5080](https://github.com/mozilla/application-services/pull/5080), [#7226](https://github.com/mozilla/application-services/pull/7226) |
| [Pin CI tools and actions](RULES.md#pin-ci-tools) | [#6630](https://github.com/mozilla/application-services/pull/6630), [#6634](https://github.com/mozilla/application-services/pull/6634) / [#5783](https://github.com/mozilla/application-services/pull/5783), [#6632](https://github.com/mozilla/application-services/pull/6632), [#6633](https://github.com/mozilla/application-services/pull/6633), [#6635](https://github.com/mozilla/application-services/pull/6635) |
| [Dependencies compatible with mozilla-central](RULES.md#dependency-vetting) | [#5352](https://github.com/mozilla/application-services/pull/5352), [#5731](https://github.com/mozilla/application-services/pull/5731), [#6466](https://github.com/mozilla/application-services/pull/6466) / – |
| [Components registered everywhere](RULES.md#component-wiring) | [#5531](https://github.com/mozilla/application-services/pull/5531) / [#5858](https://github.com/mozilla/application-services/pull/5858), [#6418](https://github.com/mozilla/application-services/pull/6418), [#6422](https://github.com/mozilla/application-services/pull/6422), [#7460](https://github.com/mozilla/application-services/pull/7460) |
| [`report_error!` labels / error-reporter fields](RULES.md#error-report-labels) | [#6731](https://github.com/mozilla/application-services/pull/6731), [#7335](https://github.com/mozilla/application-services/pull/7335), [#7658](https://github.com/mozilla/application-services/pull/7658) / – |
| [`unsafe` + NSS FFI](RULES.md#unsafe-and-nss) | [#6876](https://github.com/mozilla/application-services/pull/6876) / [#6997](https://github.com/mozilla/application-services/pull/6997), [#7015](https://github.com/mozilla/application-services/pull/7015), [#7308](https://github.com/mozilla/application-services/pull/7308) |
| [Schema change needs a migration](RULES.md#schema-change-needs-migration) / [`sql_support` helpers](RULES.md#use-sql-support) | [#6189](https://github.com/mozilla/application-services/pull/6189) / [#5462](https://github.com/mozilla/application-services/pull/5462), [#6456](https://github.com/mozilla/application-services/pull/6456) |
| [Unique exported names (Swift)](RULES.md#uniffi-unique-exported-names) | [#5181](https://github.com/mozilla/application-services/pull/5181), [#5293](https://github.com/mozilla/application-services/pull/5293) / – |
| [FKs declare `ON DELETE`](RULES.md#schema-constraints) | [#6016](https://github.com/mozilla/application-services/pull/6016), [#6028](https://github.com/mozilla/application-services/pull/6028) / – |
| [Unused dependencies](RULES.md#no-unused-deps) | [#6412](https://github.com/mozilla/application-services/pull/6412), [#6482](https://github.com/mozilla/application-services/pull/6482) / – |
| [Docs index / links](RULES.md#docs-index-and-adr) | [#6404](https://github.com/mozilla/application-services/pull/6404), [#6407](https://github.com/mozilla/application-services/pull/6407) / – |
| [Deterministic iteration](RULES.md#deterministic-iteration) | [#5736](https://github.com/mozilla/application-services/pull/5736) / [#5172](https://github.com/mozilla/application-services/pull/5172), [#6208](https://github.com/mozilla/application-services/pull/6208) |
| [Crate manifests](RULES.md#crate-manifest) | [#7199](https://github.com/mozilla/application-services/pull/7199), [#7209](https://github.com/mozilla/application-services/pull/7209) / – |
| [Log through `tracing_support`](RULES.md#tracing-support-logging) | [#6865](https://github.com/mozilla/application-services/pull/6865), [#7332](https://github.com/mozilla/application-services/pull/7332) / – |
| [Transaction must commit](RULES.md#transaction-must-commit) · [UTF-8-safe truncation](RULES.md#utf8-safe-truncation) · [doc comment format](RULES.md#doc-comment-format) · [mockito asserts](RULES.md#meaningful-assertions) · [actionable public errors](RULES.md#uniffi-public-errors-actionable) · [Python exit codes](RULES.md#python-ruff) · [Kotlin self-recursion](RULES.md#kotlin-gradle-misc) | [#5269](https://github.com/mozilla/application-services/pull/5269) · [#5411](https://github.com/mozilla/application-services/pull/5411) · [#6462](https://github.com/mozilla/application-services/pull/6462) · [#7347](https://github.com/mozilla/application-services/pull/7347) · [#6278](https://github.com/mozilla/application-services/pull/6278) · [#6242](https://github.com/mozilla/application-services/pull/6242) · [#6974](https://github.com/mozilla/application-services/pull/6974) |

Per-PR verdicts, root causes and proposed rules: [results/candidates/fixes_1.json](results/candidates/fixes_1.json),
[fixes_2.json](results/candidates/fixes_2.json).

## Things found while measuring

These came up while running the prototype checks on `main`. They need a second look, not alarm:

1. **ads-client cache indexes never reach older installs.** #7036 and #7088 added `CREATE INDEX IF NOT EXISTS` to
   `HttpCacheConnectionInitializer::init()` without bumping `END_VERSION`. `init()` only runs for new databases, and
   `upgrade_from(1)` only adds columns, so caches created before those PRs have no `idx_http_cache_expires_at`. Likely
   only a performance cost. (`idx_http_cache_request_hash` is also redundant with the `PRIMARY KEY`.) This is exactly
   what the [schema-version gate](RULES.md#schema-change-needs-migration) catches.
2. **relay logs through the `log` crate** (18 `log::debug!`/`trace!`/`warn!` calls). Nothing in this repo installs a `log`
   logger or bridges it to `tracing_support`, so on Android/iOS these most likely never reach the app's logger. That's
   the bug #7332 fixed for cli-support.
   The trace calls also print raw response bodies.
3. **Duplicated error-report labels:** `fxa-client-malformed-url` and `tabs-read-remote` are each used at two call
   sites, so a report can't be traced back to the code that sent it. Two labels aren't kebab-case.
4. **`filter_adult` and `breach-alerts` constructors** return `ApiResult` without `#[handle_error]`. One unwraps inside,
   the other stringifies the error (27 such fns in total).
5. **10 foreign keys in the places schema have no `ON DELETE`.** That may be deliberate for desktop parity, but #6016
   was this exact bug in suggest.

## Proposed rollout

### Step 1: configuration only

- Add [`[workspace.lints]`](prototype/workspace-lints.toml) and make crates opt in with `[lints] workspace = true` (no
  crate does today). Start with the 12 lints that have **0 hits** as `deny` (`dbg_macro`, `todo`, `print_stderr`,
  `significant_drop_in_scrutinee`, …). Add the low-count ones as `warn` (`let_underscore_must_use` 9,
  `redundant_clone` 12, `string_slice` 38, `map_err_ignore` 38, …) and flip them to deny per component as they reach 0.
  The trailing comments in that file give today's count for each lint.
- Replace the empty `clippy.toml` with [prototype/clippy.toml](prototype/clippy.toml): `allow-*-in-tests` so
  restriction lints ignore tests, `disallowed-types` (`std::sync::Mutex` 40, `RwLock` 7, `DefaultHasher` 4),
  `disallowed-macros` (`log::*` 24), `disallowed-methods` (`String::truncate` 1). It parses with our pinned toolchain
  (1.94).
- Add `typos` (spelling: 73 threads, the most of any rule), `cargo machete` (unused deps) and `ruff` for Python.
  shellcheck already runs on every `*.sh`; add a strict-mode check (17 scripts don't `set -e`) and flag `sed -i`, the
  GNU/BSD difference behind #6849.
- `cargo doc -D rustdoc::broken_intra_doc_links` so renames break stale docs.

### Step 2: a small `repo-lint` job (scripts and ast-grep)

[prototype/](prototype/) has a working start: 8 [ast-grep rules](prototype/rules/) and
[checks.py](prototype/checks.py) with 14 checks. Today they find:

| Check | Hits on `main` |
|---|---:|
| UDL declarations / proc-macro exports without `///` | 398 / 87 |
| TODO/FIXME without a bug link | 122 |
| possible PII in log/report macros (heuristic) | 76 |
| commented-out code (heuristic) | 52 |
| `INSERT OR REPLACE` (scope to FK-parent tables) | 44 |
| files without MPL header / final newline | 36 / 11 |
| `fn … -> ApiResult` without `#[handle_error]` | 27 |
| `static` Mutex/RwLock caches | 12 |
| FKs without `ON DELETE` | 10 |
| transactions never committed (all read-only today) | 4 |
| duplicate / malformed `report_error!` labels | 4 |
| duplicate exported UniFFI names (iOS megazord) | 0 |
| `Arc::try_unwrap(x.clone())` | 0 |

Run it on changed lines only at first, non-blocking. Promote checks to blocking once their noise is known.

Backtest of the diff-based gates on the 1,028 PRs merged since 2025-01-01
([scripts/backtest.py](scripts/backtest.py)):

| Gate | PRs it applies to | Would have flagged | Verdict |
|---|---:|---:|---|
| Schema DDL changed without bumping `VERSION`/`END_VERSION` | 31 | 5 (2 real misses) | precise, can block |
| ≥20 new src lines and no test changes | 359 | 95 (26%) | warning only |
| Consumer-visible change without CHANGELOG.md | 603 | 381 (63%) | too noisy to block; warn, with a `skip-changelog` label |
| Removed export line without "breaking" in CHANGELOG (naive grep) | 156 | 136 (87%) | useless as a grep; needs the API snapshot (step 3) |

### Step 3: UniFFI-aware tooling

1. **API snapshot + breaking-change gate** (rule #1 overall).
2. **Record checker:** `Option` fields without `#[uniffi(default = None)]`, new non-defaulted fields, `Option<Vec<_>>`,
   defaults that differ from `impl Default`.
3. **Panic-free lifts:** no `unwrap`/`expect`/`panic!` in `FfiConverter`/`custom_type!` conversions and exported
   constructors.
4. **Callback interfaces:** methods return `Result` with an `UnexpectedUniFFICallbackError` conversion; stores release
   callbacks on shutdown.
5. **Compile wrappers on PRs** whenever UDL, exports or `android/`/`ios/` change, with Swift warnings as errors.

Several of these could live in UniFFI itself or in a shared `uniffi-lint` crate other UniFFI users could adopt.

### Step 4: what stays human

The 9 tier-3 rules and the non-lintable themes the agents saw most often belong in a short review guide (or the PR
template) rather than a linter:

- **naming and API clarity:** the largest non-lintable bucket, ~90 threads
- **comments that explain *why*:** ~65
- **tests that assert the specific outcome and cover the failure path:** ~45
- **module placement / layering**
- **coordinating consumer PRs before landing**
- **splitting refactors from behaviour changes**
- **domain semantics:** sync engines, FxA state machine, Remote Settings

## Reproduce

See [README.md](README.md). Everything except the LLM extraction step is deterministic; the extraction prompts are in
[prompts/](prompts/) and their raw outputs in [results/candidates/](results/candidates/).
