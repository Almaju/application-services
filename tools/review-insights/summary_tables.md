## top

| # | Rule | Area | Threads (≥ PRs) | Periods | Bugfix PRs | Tier |
|---:|---|---|---:|---:|---:|---|
| 1 | [Detect binding API changes and require a ⚠️ Breaking Changes entry](RULES.md#uniffi-api-break-gate) | UniFFI & bindings | 62 (≥41) | 7/7 | 2 | 2 |
| 2 | [No `unwrap()`/`expect()`/`panic!` in non-test component code](RULES.md#no-unwrap-in-component-code) | Errors & panics | 46 (≥23) | 6/7 | 8 | 0 |
| 3 | [Spell-check identifiers, comments, docs, UDL and CHANGELOG](RULES.md#spellcheck) | Docs, comments & CHANGELOG | 73 (≥38) | 7/7 |  | 0 |
| 4 | [Use Option/Result combinators and `let-else` instead of manual matches](RULES.md#option-result-combinators) | Rust idioms (clippy config) | 60 (≥34) | 6/7 |  | 0 |
| 5 | [Every source file starts with the MPL-2.0 header and ends with a newline](RULES.md#license-header-and-whitespace) | Docs, comments & CHANGELOG | 28 (≥22) | 5/7 | 6 | 1 |
| 6 | [Avoid needless clones and allocations; write `Arc::clone(&x)`](RULES.md#needless-clones) | Rust idioms (clippy config) | 54 (≥32) | 7/7 |  | 0 |
| 7 | [Any schema change bumps `VERSION`, adds an `upgrade_from` step and an upgrade test](RULES.md#schema-change-needs-migration) | SQL, storage & migrations | 39 (≥25) | 7/7 | 3 | 1 |
| 8 | [Doc comments reference things that exist (intra-doc links, stale names)](RULES.md#docs-in-sync) | Docs, comments & CHANGELOG | 56 (≥32) | 6/7 |  | 0 |
| 9 | [Descriptive identifiers: no cryptic abbreviations, follow Rust API naming](RULES.md#naming) | API design & naming | 50 (≥32) | 6/7 |  | 1 |
| 10 | [Keep public error enums small and actionable; one reported catch-all with a reason](RULES.md#uniffi-public-errors-actionable) | UniFFI & bindings | 39 (≥28) | 7/7 | 1 | 3 |
| 11 | [A small curated pedantic subset (derives, unit structs, `Default` use)](RULES.md#pedantic-subset) | Rust idioms (clippy config) | 52 (≥31) | 6/7 |  | 0 |
| 12 | [Every exported item has a `///` docstring (it is what Kotlin/Swift consumers see)](RULES.md#uniffi-docstrings) | UniFFI & bindings | 50 (≥29) | 6/7 |  | 1 |
| 13 | [New/bumped crates must be vetted and compatible with mozilla-central](RULES.md#dependency-vetting) | Dependencies, build & component wiring | 25 (≥22) | 6/7 | 3 | 1 |
| 14 | [CHANGELOG edits go in the in-progress section, under a component heading, with a PR link](RULES.md#changelog-format) | Docs, comments & CHANGELOG | 36 (≥26) | 6/7 |  | 1 |
| 15 | [Let rustfmt own imports and layout (incl. inside macros)](RULES.md#rustfmt-config) | Rust idioms (clippy config) | 44 (≥26) | 6/7 |  | 0 |
| 16 | [Every PR compiles all targets that a change can break (wrappers, examples, tests, taskgraph)](RULES.md#compile-everything-on-pr) | Dependencies, build & component wiring | 9 (≥8) | 4/7 | 10 | 1 |
| 17 | [New/renamed components are registered everywhere (megazords, uniffi.toml, Swift, Gradle, CI)](RULES.md#component-wiring) | Dependencies, build & component wiring | 25 (≥14) | 5/7 | 5 | 1 |
| 18 | [Name and explain magic values and non-obvious intent](RULES.md#explain-magic-values) | Docs, comments & CHANGELOG | 34 (≥22) | 6/7 |  | 3 |
| 19 | [Tests are isolated: no wall clock, shared global state, fixed DB paths or prod endpoints](RULES.md#test-isolation) | Tests | 8 (≥7) | 3/7 | 9 | 1 |
| 20 | [Use the narrowest visibility (`pub(crate)`/private)](RULES.md#minimal-visibility) | API design & naming | 31 (≥21) | 6/7 |  | 0 |
| 21 | [New behaviour comes with tests in the same PR](RULES.md#tests-with-behaviour) | Tests | 46 (≥23) | 4/7 |  | 1 |
| 22 | [Don't silently discard errors (`let _ =`, `.ok()`, `if let Ok`, `_ => None`)](RULES.md#dont-swallow-errors) | Errors & panics | 36 (≥21) | 5/7 |  | 0 |
| 23 | [Remove unused pub items, error variants and columns that rustc can't see](RULES.md#remove-dead-pub-code) | Dead code & leftovers | 25 (≥20) | 6/7 |  | 1 |
| 24 | [Optional/new Record fields get a UniFFI default that matches Rust's `Default`; avoid `Option<Vec<_>>`](RULES.md#uniffi-record-defaults) | UniFFI & bindings | 24 (≥19) | 4/7 | 1 | 2 |
| 25 | [Types deserialized from server/Remote Settings data tolerate absent and unknown values](RULES.md#tolerant-remote-deserialization) | Remote data & serde compatibility | 25 (≥18) | 4/7 | 1 | 1 |

## uniffi

| Rule | Threads (≥ PRs) | Bugfix PRs | Tier | Today |
|---|---:|---:|---|---|
| [Detect binding API changes and require a ⚠️ Breaking Changes entry](RULES.md#uniffi-api-break-gate) | 62 (≥41) | 2 | 2 | a naive diff heuristic (removed export lines without a "breaking" CHANGELOG line) flags 136 of 156 API-touching PRs since 2025, which is too noisy; this needs a real API snapshot, not a grep |
| [Keep public error enums small and actionable; one reported catch-all with a reason](RULES.md#uniffi-public-errors-actionable) | 39 (≥28) | 1 | 3 |  |
| [Every exported item has a `///` docstring (it is what Kotlin/Swift consumers see)](RULES.md#uniffi-docstrings) | 50 (≥29) |  | 1 | 398 UDL declarations/methods without `///`, 87 proc-macro exported types/fns without `///` (checks.py); `missing_docs`: 4357 pub items |
| [Optional/new Record fields get a UniFFI default that matches Rust's `Default`; avoid `Option<Vec<_>>`](RULES.md#uniffi-record-defaults) | 24 (≥19) | 1 | 2 |  |
| [Exported type names must be unique across the megazord (Swift has one flat module)](RULES.md#uniffi-unique-exported-names) | 20 (≥12) | 2 | 1 | 0 duplicates among 303 exported names in the 24 iOS-megazord crates (checks.py); 11 collision-prone generic names (`ApiError`, `Headers`, `Request`, `Store`, …) |
| [Expose typed values over FFI (enums, Url, timestamps), not strings, magic ints or raw JSON](RULES.md#uniffi-typed-values) | 23 (≥16) |  | 3 |  |
| [Exported names must read well in Kotlin/Swift and say what they do](RULES.md#uniffi-names-read-well) | 16 (≥14) |  | 3 |  |
| [Never panic in lifts, custom-type conversions or exported constructors](RULES.md#uniffi-no-panic-across-ffi) | 5 (≥4) | 5 | 1 |  |
| [Keep the FFI layer thin: `Arc`, builders and serde stay at the boundary](RULES.md#uniffi-thin-ffi-layer) | 14 (≥9) |  | 3 |  |
| [Exported fallible functions return `ApiResult` via `#[handle_error]`; internal code uses the crate `Error`](RULES.md#uniffi-handle-error) | 13 (≥8) |  | 1 | 27 fns return `ApiResult` without `#[handle_error]` (checks.py) |
| [Callback interfaces return `Result`, are released on shutdown, and user-waiting calls are async](RULES.md#uniffi-callback-contract) | 6 (≥3) | 2 | 2 |  |
| [Don't export test-only helpers over FFI](RULES.md#uniffi-no-test-only-exports) | 7 (≥5) |  | 1 |  |
| [UDL and proc-macro declarations agree; new code uses proc-macros](RULES.md#uniffi-udl-proc-macro-consistency) | 12 (≥5) |  | 1 |  |

## tiers

| Tier | Rules | Review threads covered |
|---|---:|---:|
| Tier 0 · config only | 29 | 727 |
| Tier 1 · small script / ast-grep | 46 | 833 |
| Tier 2 · custom tooling | 7 | 139 |
| Tier 3 · review checklist | 9 | 170 |

## fixes

| Rule | Bugfix PRs (preventable / partially) |
|---|---|
| [No `unwrap()`/`expect()`/`panic!` in non-test component code](RULES.md#no-unwrap-in-component-code) | [#6967](https://github.com/mozilla/application-services/pull/6967), [#6971](https://github.com/mozilla/application-services/pull/6971), [#6972](https://github.com/mozilla/application-services/pull/6972), [#6973](https://github.com/mozilla/application-services/pull/6973), [#7273](https://github.com/mozilla/application-services/pull/7273), [#7536](https://github.com/mozilla/application-services/pull/7536), [#7578](https://github.com/mozilla/application-services/pull/7578) / [#6143](https://github.com/mozilla/application-services/pull/6143) |
| [Every PR compiles all targets that a change can break (wrappers, examples, tests, taskgraph)](RULES.md#compile-everything-on-pr) | [#5223](https://github.com/mozilla/application-services/pull/5223), [#5259](https://github.com/mozilla/application-services/pull/5259), [#5272](https://github.com/mozilla/application-services/pull/5272), [#5536](https://github.com/mozilla/application-services/pull/5536), [#7437](https://github.com/mozilla/application-services/pull/7437) / [#4920](https://github.com/mozilla/application-services/pull/4920), [#5610](https://github.com/mozilla/application-services/pull/5610), [#5689](https://github.com/mozilla/application-services/pull/5689), [#5695](https://github.com/mozilla/application-services/pull/5695), [#7226](https://github.com/mozilla/application-services/pull/7226) |
| [Every source file starts with the MPL-2.0 header and ends with a newline](RULES.md#license-header-and-whitespace) | [#6243](https://github.com/mozilla/application-services/pull/6243), [#6514](https://github.com/mozilla/application-services/pull/6514), [#6515](https://github.com/mozilla/application-services/pull/6515), [#6623](https://github.com/mozilla/application-services/pull/6623), [#6626](https://github.com/mozilla/application-services/pull/6626), [#6903](https://github.com/mozilla/application-services/pull/6903) / – |
| [Never panic in lifts, custom-type conversions or exported constructors](RULES.md#uniffi-no-panic-across-ffi) | [#6967](https://github.com/mozilla/application-services/pull/6967), [#6971](https://github.com/mozilla/application-services/pull/6971), [#6972](https://github.com/mozilla/application-services/pull/6972), [#6973](https://github.com/mozilla/application-services/pull/6973), [#7578](https://github.com/mozilla/application-services/pull/7578) / – |
| [Tests are isolated: no wall clock, shared global state, fixed DB paths or prod endpoints](RULES.md#test-isolation) | – / [#5385](https://github.com/mozilla/application-services/pull/5385), [#5403](https://github.com/mozilla/application-services/pull/5403), [#5449](https://github.com/mozilla/application-services/pull/5449), [#5785](https://github.com/mozilla/application-services/pull/5785), [#6032](https://github.com/mozilla/application-services/pull/6032), [#6033](https://github.com/mozilla/application-services/pull/6033), [#6126](https://github.com/mozilla/application-services/pull/6126), [#6728](https://github.com/mozilla/application-services/pull/6728), [#6730](https://github.com/mozilla/application-services/pull/6730) |
| [Shell scripts use `set -euo pipefail` and stay portable (shellcheck alone misses both)](RULES.md#shell-strict-shellcheck) | [#6392](https://github.com/mozilla/application-services/pull/6392), [#6512](https://github.com/mozilla/application-services/pull/6512), [#6849](https://github.com/mozilla/application-services/pull/6849) / [#5080](https://github.com/mozilla/application-services/pull/5080), [#7226](https://github.com/mozilla/application-services/pull/7226) |
| [Pin CI tools, images and actions (no `:latest`, current major of actions)](RULES.md#pin-ci-tools) | [#6630](https://github.com/mozilla/application-services/pull/6630), [#6634](https://github.com/mozilla/application-services/pull/6634) / [#5783](https://github.com/mozilla/application-services/pull/5783), [#6632](https://github.com/mozilla/application-services/pull/6632), [#6633](https://github.com/mozilla/application-services/pull/6633), [#6635](https://github.com/mozilla/application-services/pull/6635) |
| [New/bumped crates must be vetted and compatible with mozilla-central](RULES.md#dependency-vetting) | [#5352](https://github.com/mozilla/application-services/pull/5352), [#5731](https://github.com/mozilla/application-services/pull/5731), [#6466](https://github.com/mozilla/application-services/pull/6466) / – |
| [New/renamed components are registered everywhere (megazords, uniffi.toml, Swift, Gradle, CI)](RULES.md#component-wiring) | [#5531](https://github.com/mozilla/application-services/pull/5531) / [#5858](https://github.com/mozilla/application-services/pull/5858), [#6418](https://github.com/mozilla/application-services/pull/6418), [#6422](https://github.com/mozilla/application-services/pull/6422), [#7460](https://github.com/mozilla/application-services/pull/7460) |
| [`report_error!` labels are component-prefixed, unique, and not on hot paths](RULES.md#error-report-labels) | [#6731](https://github.com/mozilla/application-services/pull/6731), [#7335](https://github.com/mozilla/application-services/pull/7335), [#7658](https://github.com/mozilla/application-services/pull/7658) / – |
| [Every `unsafe` block has a `// SAFETY:` comment; NSS allocations use RAII](RULES.md#unsafe-and-nss) | [#6876](https://github.com/mozilla/application-services/pull/6876) / [#6997](https://github.com/mozilla/application-services/pull/6997), [#7015](https://github.com/mozilla/application-services/pull/7015), [#7308](https://github.com/mozilla/application-services/pull/7308) |
| [Any schema change bumps `VERSION`, adds an `upgrade_from` step and an upgrade test](RULES.md#schema-change-needs-migration) | [#6189](https://github.com/mozilla/application-services/pull/6189) / [#5462](https://github.com/mozilla/application-services/pull/5462), [#6456](https://github.com/mozilla/application-services/pull/6456) |
| [No unused or misplaced dependencies](RULES.md#no-unused-deps) | [#6412](https://github.com/mozilla/application-services/pull/6412), [#6482](https://github.com/mozilla/application-services/pull/6482) / – |
| [Exported type names must be unique across the megazord (Swift has one flat module)](RULES.md#uniffi-unique-exported-names) | [#5181](https://github.com/mozilla/application-services/pull/5181), [#5293](https://github.com/mozilla/application-services/pull/5293) / – |
| [Columns declare NOT NULL/keys, FKs declare ON DELETE and are indexed, new indexes are justified](RULES.md#schema-constraints) | [#6016](https://github.com/mozilla/application-services/pull/6016), [#6028](https://github.com/mozilla/application-services/pull/6028) / – |
| [New docs are linked from SUMMARY.md; ADRs have complete metadata; links are permalinks](RULES.md#docs-index-and-adr) | [#6404](https://github.com/mozilla/application-services/pull/6404), [#6407](https://github.com/mozilla/application-services/pull/6407) / – |
| [Don't let HashMap/HashSet order leak into results, storage or generated output](RULES.md#deterministic-iteration) | [#5736](https://github.com/mozilla/application-services/pull/5736) / [#5172](https://github.com/mozilla/application-services/pull/5172), [#6208](https://github.com/mozilla/application-services/pull/6208) |
| [New crate manifests: license, description, `exclude` platform dirs, no deps on `examples/`](RULES.md#crate-manifest) | [#7199](https://github.com/mozilla/application-services/pull/7199), [#7209](https://github.com/mozilla/application-services/pull/7209) / – |
| [Log through `tracing_support`, not the `log` crate](RULES.md#tracing-support-logging) | [#6865](https://github.com/mozilla/application-services/pull/6865), [#7332](https://github.com/mozilla/application-services/pull/7332) / – |
| [Python automation is formatted and linted (ruff) and exits non-zero on failure](RULES.md#python-ruff) | [#6242](https://github.com/mozilla/application-services/pull/6242) / [#6369](https://github.com/mozilla/application-services/pull/6369) |
| [Kotlin/Gradle hygiene: injectable dispatchers, KDoc params, lazy task APIs, no lint baselines](RULES.md#kotlin-gradle-misc) | [#6974](https://github.com/mozilla/application-services/pull/6974) / [#5733](https://github.com/mozilla/application-services/pull/5733) |
| [Detect binding API changes and require a ⚠️ Breaking Changes entry](RULES.md#uniffi-api-break-gate) | – / [#4975](https://github.com/mozilla/application-services/pull/4975), [#6927](https://github.com/mozilla/application-services/pull/6927) |
| [Keep public error enums small and actionable; one reported catch-all with a reason](RULES.md#uniffi-public-errors-actionable) | [#6278](https://github.com/mozilla/application-services/pull/6278) / – |
| [Build URLs with `url::Url`, never by string concat; no hard-coded endpoints](RULES.md#url-hygiene) | – / [#7046](https://github.com/mozilla/application-services/pull/7046), [#7543](https://github.com/mozilla/application-services/pull/7543) |
| [Well-formed doc comments: `///` for items, `//!` for modules, fenced examples](RULES.md#doc-comment-format) | [#6462](https://github.com/mozilla/application-services/pull/6462) / – |
| [No wildcard arms on domain enums; destructure fully in merge/hash/eq impls](RULES.md#exhaustive-matches) | – / [#6889](https://github.com/mozilla/application-services/pull/6889), [#7310](https://github.com/mozilla/application-services/pull/7310) |
| [Use `sql_support` helpers (`ConnExt`, cached statements, `ConnectionInitializer`) instead of raw rusqlite](RULES.md#use-sql-support) | [#6189](https://github.com/mozilla/application-services/pull/6189) / – |
| [Tests must actually assert: no bare `matches!`, mockito mocks asserted, messages match](RULES.md#meaningful-assertions) | [#7347](https://github.com/mozilla/application-services/pull/7347) / – |
| [Sync payloads round-trip unknown fields and tolerate nulls](RULES.md#sync-payload-compat) | – / [#5315](https://github.com/mozilla/application-services/pull/5315), [#5555](https://github.com/mozilla/application-services/pull/5555) |
| [Callback interfaces return `Result`, are released on shutdown, and user-waiting calls are async](RULES.md#uniffi-callback-contract) | – / [#7478](https://github.com/mozilla/application-services/pull/7478), [#7520](https://github.com/mozilla/application-services/pull/7520) |
| [Every transaction bound to a variable is committed (drop = silent rollback)](RULES.md#transaction-must-commit) | [#5269](https://github.com/mozilla/application-services/pull/5269) / – |
| [Never truncate or slice strings at byte offsets](RULES.md#utf8-safe-truncation) | [#5411](https://github.com/mozilla/application-services/pull/5411) / – |
| [Optional/new Record fields get a UniFFI default that matches Rust's `Default`; avoid `Option<Vec<_>>`](RULES.md#uniffi-record-defaults) | – / [#6387](https://github.com/mozilla/application-services/pull/6387) |
| [Types deserialized from server/Remote Settings data tolerate absent and unknown values](RULES.md#tolerant-remote-deserialization) | – / [#7676](https://github.com/mozilla/application-services/pull/7676) |
| [Never call foreign callbacks, do network I/O, or re-lock while holding a store lock](RULES.md#no-callbacks-or-io-under-lock) | – / [#6828](https://github.com/mozilla/application-services/pull/6828) |
| [No panicking indexing/slicing or unchecked arithmetic on external sizes and timestamps](RULES.md#no-panicking-index-or-arith) | – / [#6125](https://github.com/mozilla/application-services/pull/6125) |
| [Validate metrics.yaml: bugs, data reviews, notification emails, expiry](RULES.md#glean-metrics-yaml) | – / [#5069](https://github.com/mozilla/application-services/pull/5069) |
| [Keep error sources typed (`#[from]`), not stringified or mapped to an unrelated variant](RULES.md#preserve-error-sources) | – / [#6732](https://github.com/mozilla/application-services/pull/6732) |
| [No URLs, titles, usernames or record payloads in logs, errors or reports](RULES.md#no-pii-in-logs) | – / [#7549](https://github.com/mozilla/application-services/pull/7549) |
| [Check the HTTP status before using a response body; only explicit success is success](RULES.md#http-status-checked) | – / [#6819](https://github.com/mozilla/application-services/pull/6819) |
