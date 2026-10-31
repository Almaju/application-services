"""Canonical rules: each one merges the near-duplicate candidates the
per-chunk extraction produced (`cN:` = review chunk N, `fN:` = bugfix batch N).

tier: 0 = turn on an existing lint/formatter setting
      1 = small script / ast-grep / regex check, or a CI diff check
      2 = custom tooling (UniFFI-aware checker, API snapshots, dylint)
      3 = review checklist (not reliably automatable)
"""

GROUPS = {
    "uniffi": "UniFFI & bindings",
    "errors": "Errors & panics",
    "sql": "SQL, storage & migrations",
    "remote": "Remote data & serde compatibility",
    "concurrency": "Concurrency & callbacks",
    "privacy": "Logging & privacy",
    "docs": "Docs, comments & CHANGELOG",
    "deadcode": "Dead code & leftovers",
    "idioms": "Rust idioms (clippy config)",
    "api": "API design & naming",
    "deps": "Dependencies, build & component wiring",
    "tests": "Tests",
    "scripts": "Scripts & CI",
    "mobile": "Kotlin / Swift wrappers",
    "nimbus-glean": "Nimbus FML & Glean",
}

CANON = [
    # ------------------------------------------------------------------ uniffi
    dict(id="uniffi-api-break-gate", group="uniffi", tier=2,
         title="Detect binding API changes and require a ⚠️ Breaking Changes entry",
         how="Check in a per-component API snapshot (UniFFI metadata from `uniffi-bindgen` library mode, plus Kotlin "
             "`binary-compatibility-validator` `.api` files and a Swift API digest for hand-written wrappers). CI diffs "
             "the snapshot against `main`; removed/renamed items, changed signatures, new non-defaulted args/fields, or "
             "newly `@Deprecated` API fail unless CHANGELOG.md's in-progress section has a `### ⚠️ Breaking Changes ⚠️` "
             "bullet for that component.",
         members=["c3:breaking-change-declared", "c6:no-unflagged-breaking-binding-changes", "c1:uniffi-breaking-change-detection",
                  "c4:uniffi-breaking-change-needs-changelog-and-consumer-prs", "c7:uniffi-breaking-change-needs-changelog-and-consumer-prs",
                  "c5:uniffi-detect-breaking-binding-changes", "c2:changelog-breaking-change-entry", "f2:public-api-diff",
                  "f1:deprecation-is-breaking", "c1:wrapper-public-api-compat", "c2:new-params-appended-with-defaults"]),
    dict(id="uniffi-unique-exported-names", group="uniffi", tier=1,
         title="Exported type names must be unique across the megazord (Swift has one flat module)",
         how="Script: collect every exported name (UDL `dictionary`/`interface`/`enum`/`[Error]`/`callback interface`, "
             "`#[derive(uniffi::Record|Enum|Object|Error)]`, `#[uniffi::export] trait`, top-level exported fns) for each "
             "component in `megazords/ios-rust` and `megazords/full`, and fail on duplicates. New components should "
             "prefix generic names (`Error`, `Config`, `Record`, `Metadata`).",
         members=["c6:uniffi-exported-type-names-prefixed", "c3:uniffi-unique-prefixed-type-names",
                  "c5:uniffi-unique-type-names-across-components", "c1:uniffi-unique-type-names-across-megazord",
                  "f2:uniffi-swift-name-uniqueness"]),
    dict(id="uniffi-docstrings", group="uniffi", tier=1,
         title="Every exported item has a `///` docstring (it is what Kotlin/Swift consumers see)",
         how="`#![warn(missing_docs)]` on component crates catches `pub` Rust items. A small checker covers what "
             "rustc can't: UDL declarations and members not preceded by `///` (a `//` comment is dropped from the "
             "bindings), `#[uniffi::export]` fns/methods and `uniffi::Record` fields without docs, and callback "
             "interface methods without a threading/blocking note.",
         members=["c2:doc-comments-on-exported-api", "c6:uniffi-exported-items-documented", "c5:uniffi-exported-items-require-docstrings",
                  "c4:udl-docstrings-use-triple-slash", "c3:callback-interface-contract-docs", "c4:docs-on-public-items",
                  "c3:rustdoc-on-public-items", "c1:document-public-and-exported-items"]),
    dict(id="uniffi-record-defaults", group="uniffi", tier=2,
         title="Optional/new Record fields get a UniFFI default that matches Rust's `Default`; avoid `Option<Vec<_>>`",
         how="UniFFI checker over `uniffi::Record`/UDL dictionaries: `Option<T>` fields without `#[uniffi(default = None)]` "
             "(or UDL `= null`), new fields without a default (breaking for Kotlin/Swift callers), `Option<Vec<T>>` / "
             "`Option<HashMap<..>>` (use an empty collection), and `#[uniffi(default)]` values that differ from the "
             "`impl Default`.",
         members=["c2:udl-optional-fields-need-defaults", "f2:uniffi-option-defaults", "c5:uniffi-record-defaults-and-no-option-vec",
                  "c7:uniffi-record-defaults-match-rust-default", "c3:uniffi-no-gratuitous-optionals",
                  "c2:avoid-option-wrapped-collections", "c2:no-always-some-option-params"]),
    dict(id="uniffi-handle-error", group="uniffi", tier=1,
         title="Exported fallible functions return `ApiResult` via `#[handle_error]`; internal code uses the crate `Error`",
         how="ast-grep: `#[uniffi::export]` fn/method returning `Result<_, _>` must carry `#[handle_error(..)]` and "
             "return the component's `ApiResult`; non-exported fns returning `ApiResult`/`*ApiError` are flagged; "
             "constructors/functions that parse input must be fallible; no `impl From<X> for *ApiError` (go through "
             "`GetErrorHandling`).",
         members=["c1:exported-fns-return-apiresult-with-handle-error", "c7:handle-error-on-exported-apiresult-fns",
                  "c4:internal-fns-return-internal-error", "c2:uniffi-fallible-entrypoints-must-throw",
                  "c7:no-from-impls-into-public-api-errors"]),
    dict(id="uniffi-public-errors-actionable", group="uniffi", tier=3,
         title="Keep public error enums small and actionable; one reported catch-all with a reason",
         how="Mostly review. Partially checkable: exported error variants never constructed in the crate (dead variant), "
             "variants named `*Error` inside an `*Error` enum, catch-all variants without a `reason: String`, and "
             "`get_error_handling` matches with a wildcard arm.",
         members=["c6:minimal-public-error-enums", "c3:uniffi-error-variants-actionable", "c2:uniffi-public-error-model",
                  "c1:exported-error-variants-actionable", "c2:uniffi-enum-variant-hygiene", "c4:error-variants-no-error-suffix",
                  "f2:error-catchall-has-reason", "c5:error-variants-precise-and-used", "c7:precise-error-enums"]),
    dict(id="uniffi-no-panic-across-ffi", group="uniffi", tier=1,
         title="Never panic in lifts, custom-type conversions or exported constructors",
         how="ast-grep: `unwrap`/`expect`/`panic!`/`unreachable!`/indexing inside `impl FfiConverter`, "
             "`uniffi::custom_type!`/`custom_newtype!` conversion closures, `try_lift`, `#[uniffi::constructor]` bodies "
             "and default methods of exported traits. `todo!()`/`unimplemented!()` anywhere reachable from an export.",
         members=["f1:uniffi-lift-no-panic", "c7:no-panics-reachable-from-ffi", "c4:no-todo-in-exported-code"]),
    dict(id="uniffi-callback-contract", group="uniffi", tier=2,
         title="Callback interfaces return `Result`, are released on shutdown, and user-waiting calls are async",
         how="UniFFI checker: every `#[uniffi::export(callback_interface)]`/`with_foreign` trait method returns `Result` "
             "with an error type implementing `From<UnexpectedUniFFICallbackError>`; plus a test-harness rule that "
             "stores holding foreign callbacks drop them on `shutdown()`/`close()`.",
         members=["c6:uniffi-callback-methods-return-result", "c6:foreign-trait-user-prompts-async",
                  "f1:foreign-callback-released-on-shutdown"]),
    dict(id="uniffi-no-test-only-exports", group="uniffi", tier=1,
         title="Don't export test-only helpers over FFI",
         how="Flag exported fns/methods named `*_for_test(s)`/`test_*`/`*_testing`, or with docs saying \"only for "
             "tests\", unless gated by `#[cfg(feature = \"testing\")]` / `#[cfg(test)]`.",
         members=["c6:no-test-only-api-in-public-surface", "c3:no-test-only-methods-over-ffi", "c1:test-only-code-gated"]),
    dict(id="uniffi-udl-proc-macro-consistency", group="uniffi", tier=1,
         title="UDL and proc-macro declarations agree; new code uses proc-macros",
         how="`#[uniffi::constructor]`/`#[uniffi::method]` only inside an `#[uniffi::export] impl`; new pub methods on "
             "UDL interfaces need a UDL entry (or the impl needs `#[uniffi::export]`); `[External]` typedefs must "
             "reference a real crate and correct kind; new crates shouldn't add a `.udl`.",
         members=["c6:uniffi-constructor-inside-export-impl", "c6:udl-entry-for-new-public-api",
                  "c7:udl-external-typedefs-correct-and-minimal", "c7:prefer-proc-macros-over-udl"]),
    dict(id="uniffi-names-read-well", group="uniffi", tier=3,
         title="Exported names must read well in Kotlin/Swift and say what they do",
         how="Review checklist, plus a denylist check on exported names: implementation words (`Rust`, `Impl`, `Host`, "
             "`Ffi`), Kotlin/Swift-colliding names, and destructive operations not named `delete_*`/`wipe_*`/`clear_*`.",
         members=["c4:exported-names-follow-foreign-conventions", "c3:uniffi-exported-name-hygiene",
                  "c6:destructive-api-explicit-names", "c3:naming-consistent-across-platforms"]),
    dict(id="uniffi-typed-values", group="uniffi", tier=3,
         title="Expose typed values over FFI (enums, Url, timestamps), not strings, magic ints or raw JSON",
         how="Review, with heuristics: exported `String` fields named `*_json`/`*_type`/`*_kind`/`*_url`/`*_status`, "
             "`i64`/`u64` fields named `*_ms`/`*_secs` without a newtype, and `bool` pairs.",
         members=["c6:typed-ffi-fields", "c3:uniffi-typed-params-not-strings", "c5:use-enums-not-strings-magic-ints-or-bool-pairs",
                  "c2:uniffi-json-custom-types"]),
    dict(id="uniffi-thin-ffi-layer", group="uniffi", tier=3,
         title="Keep the FFI layer thin: `Arc`, builders and serde stay at the boundary",
         how="Review, with heuristics: `Arc<T>`/`Arc<Self>` parameters on non-exported fns, builder methods that "
             "consume/`take()` state, `#[derive(Serialize, Deserialize, uniffi::Record)]` on the same type, core modules "
             "importing the crate's FFI types.",
         members=["c5:uniffi-arc-only-at-ffi-boundary", "c4:uniffi-builders-non-consuming",
                  "c6:builder-or-config-for-growing-constructors", "c7:no-serde-derive-on-exported-records",
                  "c7:ffi-layer-stays-thin"]),
    # ------------------------------------------------------------------ errors
    dict(id="no-unwrap-in-component-code", group="errors", tier=0,
         title="No `unwrap()`/`expect()`/`panic!` in non-test component code",
         how="`clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic` (restriction) at `warn` in component crates, "
             "with `allow-unwrap-in-tests`/`allow-expect-in-tests`/`allow-panic-in-tests = true` in clippy.toml. "
             "Justified cases use `#[expect(clippy::expect_used, reason = \"…\")]`.",
         members=["c2:no-unwrap-expect-in-library-code", "c5:no-unwrap-expect-panic-in-component-code",
                  "c1:no-unwrap-in-library-code", "c3:no-unwrap-in-non-test-code", "c4:no-unwrap-in-non-test-code",
                  "f2:no-unwrap-on-external-data", "f1:clippy-no-unwrap-in-component-code", "c6:no-panics-in-component-code",
                  "c4:expect-message-describes-failure", "c2:remote-settings-response-handling"]),
    dict(id="no-panicking-index-or-arith", group="errors", tier=0,
         title="No panicking indexing/slicing or unchecked arithmetic on external sizes and timestamps",
         how="`clippy::indexing_slicing`, `clippy::arithmetic_side_effects` (scoped to parsing/JEXL/TTL modules via "
             "`#![warn]`), `clippy::cast_possible_truncation`/`cast_sign_loss` for `as` casts.",
         members=["c4:no-panicking-index-or-slice", "c6:no-panicking-slice-index", "c6:checked-arithmetic-on-sizes",
                  "f2:panic-free-jexl-transforms", "c7:checked-arithmetic-on-timestamps-and-config-values",
                  "c4:no-lossy-as-casts"]),
    dict(id="utf8-safe-truncation", group="errors", tier=0,
         title="Never truncate or slice strings at byte offsets",
         how="`clippy::string_slice` (restriction) plus `disallowed-methods = [\"alloc::string::String::truncate\"]` "
             "pointing to a shared char-boundary-safe helper.",
         members=["c2:utf8-safe-string-truncation", "f2:no-byte-index-string-truncation"]),
    dict(id="dont-swallow-errors", group="errors", tier=0,
         title="Don't silently discard errors (`let _ =`, `.ok()`, `if let Ok`, `_ => None`)",
         how="`clippy::let_underscore_must_use` catches `let _ = fallible()`; `clippy::map_err_ignore` catches "
             "`map_err(|_| …)`. An ast-grep rule flags `.ok()` / `.ok().flatten()` / `filter_map(|x| x.ok())` on "
             "`Result`s without a nearby `warn!`/`report_error!`.",
         members=["c7:dont-swallow-errors", "c6:no-swallowed-errors", "c4:no-let-underscore-on-result",
                  "c2:no-silent-data-drop", "c6:log-on-silent-fallback", "c5:report-swallowed-errors-via-error-support"]),
    dict(id="preserve-error-sources", group="errors", tier=1,
         title="Keep error sources typed (`#[from]`), not stringified or mapped to an unrelated variant",
         how="ast-grep: `map_err(|e| X(e.to_string()))`, `map_err(|_| …)`, manual `From` impls that format the "
             "source, and a blanket `#[from] std::io::Error` in component error enums.",
         members=["c5:errors-wrap-sources-with-from", "c4:typed-error-variants-preserve-source", "f1:no-blanket-from-io-error",
                  "c3:error-context-on-user-input-parsing", "c6:no-map-err-discarding-source"]),
    dict(id="error-report-labels", group="errors", tier=1,
         title="`report_error!` labels are component-prefixed, unique, and not on hot paths",
         how="Regex over `report_error!(\"…\")`/`breadcrumb!`: label must match `^<component>-[a-z0-9-]+$` and be "
             "unique across the repo; use `error_support` reporting instead of `log::error!` for unexpected states; "
             "flag reporting inside per-record loops.",
         members=["c1:report-error-class-naming", "f1:report-error-label-prefix", "c6:unique-specific-error-report-labels",
                  "c1:use-error-support-reporting", "c2:error-reporting-noise", "f1:error-reporter-event-schema"]),
    dict(id="http-status-checked", group="errors", tier=1,
         title="Check the HTTP status before using a response body; only explicit success is success",
         how="ast-grep: `.json()`/`.text()`/`.body` on a viaduct `Response` without a preceding `require_success()`/"
             "`is_success()` in the same fn; `status >= 400`-style checks.",
         members=["f1:check-http-status-before-body", "c6:http-status-explicit-success"]),
    # --------------------------------------------------------------------- sql
    dict(id="schema-change-needs-migration", group="sql", tier=1,
         title="Any schema change bumps `VERSION`, adds an `upgrade_from` step and an upgrade test",
         how="CI diff check: if a PR changes `CREATE TABLE|INDEX|TRIGGER|VIEW` SQL in `components/*/src/**/schema.rs` "
             "(or `*.sql`) it must also change the `VERSION` constant and touch `upgrade_from`; a shared "
             "`sql_support` test harness opens the oldest fixture, upgrades step by step and compares the result with "
             "a freshly created schema.",
         members=["c2:schema-change-requires-migration", "c6:schema-change-requires-migration-and-cleanup",
                  "c4:schema-change-requires-version-bump", "c7:sql-schema-bump-needs-upgrade-test-and-backfill",
                  "c5:sql-schema-change-requires-migration", "c1:sql-schema-migration-checklist", "c3:sql-schema-version-bump",
                  "c4:migrations-targeted-and-tested", "f2:sql-migration-harness", "c6:nimbus-rkv-migrations-chain",
                  "c6:sql-migration-upgrade-from-arms", "c5:migration-fn-naming-upgrade-from"]),
    dict(id="migration-code-frozen", group="sql", tier=1,
         title="Migrations use frozen SQL and fixtures; tests assert the `VERSION` constant",
         how="ast-grep/regex: `upgrade_from` bodies must not reference the live `CREATE_*_SQL` constants; historical "
             "schema fixtures under `sql/` or `tests/` are append-only (CI fails if an existing one is modified); "
             "migration tests compare `user_version` to the crate's `VERSION`, not a literal; DDL uses `IF [NOT] EXISTS`.",
         members=["c7:migration-tests-use-version-constants", "c6:frozen-historical-schema-fixtures",
                  "c7:sql-upgrades-use-frozen-schema", "c6:sql-migration-idempotent-ddl", "c3:persisted-state-serde-migration",
                  "c7:persisted-type-change-needs-bw-compat-test"]),
    dict(id="transaction-must-commit", group="sql", tier=1,
         title="Every transaction bound to a variable is committed (drop = silent rollback)",
         how="ast-grep: `let $TX = $C.(unchecked_)transaction(..)` with no `$TX.commit()` later in the same fn; "
             "multi-statement writes outside a transaction.",
         members=["c1:sql-transaction-must-commit", "f2:sql-transaction-must-commit",
                  "c5:sql-one-transaction-per-multi-step-operation"]),
    dict(id="sqlite-footguns", group="sql", tier=1,
         title="SQLite footguns: `INSERT OR REPLACE`, `COUNT(*)` for existence, `LIKE` prefix, unsafe `LOWER()`",
         how="Regex over SQL string literals: `INSERT OR REPLACE`/`REPLACE INTO` (deletes the conflicting row and its "
             "cascades; use `ON CONFLICT … DO UPDATE`), `SELECT COUNT(*)` compared to 0 (use `EXISTS`), `LIKE :x || '%'` "
             "(not index-friendly), `LOWER()/UPPER()` on user text, `ORDER BY` a nullable column without `NULLS`/direction.",
         members=["c5:sql-insert-or-replace-deletes-conflicting-rows", "c3:sql-sqlite-idioms", "c4:sql-exists-not-count",
                  "c4:sql-no-sqlite-case-functions", "c6:sql-prefix-match-without-like", "c4:sql-index-friendly-matching",
                  "c4:sql-order-by-null-safe-and-explicit"]),
    dict(id="schema-constraints", group="sql", tier=1,
         title="Columns declare NOT NULL/keys, FKs declare ON DELETE and are indexed, new indexes are justified",
         how="Parse `CREATE TABLE` statements (sqlparser-rs or a sqlite `PRAGMA foreign_key_list`/`index_list` test): "
             "every FK has `ON DELETE`, every FK child column is indexed, nullable columns are listed in an allowlist; "
             "new `CREATE INDEX` needs an `EXPLAIN QUERY PLAN` test.",
         members=["c3:sql-columns-not-null-and-keys", "f2:sql-fk-explicit-on-delete", "c3:sql-foreign-key-on-delete",
                  "c5:sql-index-foreign-key-child-columns", "c4:sql-nullability-matches-rust-type",
                  "c6:sql-column-names-descriptive", "c6:sql-index-justified"]),
    dict(id="sql-formatting", group="sql", tier=1,
         title="Format SQL embedded in Rust strings consistently",
         how="A formatter pass over `r#\"…\"#` strings passed to `execute*`/`query*`/`prepare*` (e.g. `sqruff`/"
             "`sqlfluff` with a repo config): uppercase keywords, one clause per line, aliases on multi-table queries.",
         members=["c4:sql-literal-formatting", "c3:sql-string-style", "c5:sql-style-uppercase-keywords-and-in-lists"]),
    dict(id="use-sql-support", group="sql", tier=1,
         title="Use `sql_support` helpers (`ConnExt`, cached statements, `ConnectionInitializer`) instead of raw rusqlite",
         how="clippy `disallowed-methods` for `rusqlite::Connection::prepare`/`execute` outside `sql_support` (suggest "
             "`prepare_cached`/`execute_cached`/`try_query_row`); multi-statement strings must go to `execute_batch`; "
             "write methods on DAOs take `&mut self`.",
         members=["c3:sql-use-sql-support-helpers", "c5:sql-use-sql_support-helpers", "c4:sql-use-connext-cached-helpers",
                  "c4:dao-over-raw-connection", "c4:dao-write-methods-take-mut-self", "f2:sql-single-statement-execute"]),
    dict(id="pragmas-centralized", group="sql", tier=1,
         title="PRAGMAs are set only in the connection initializer and documented",
         how="Regex: `PRAGMA`/`VACUUM` outside `ConnectionInitializer::prepare` implementations is flagged; any change "
             "to them requires `docs/design/db-pragmas.md` to change too.",
         members=["c7:sqlite-pragma-changes-need-review", "c1:sqlite-pragmas-centralized-documented"]),
    dict(id="no-per-item-queries", group="sql", tier=1,
         title="No per-item queries/requests in loops; long loops check for interruption",
         how="ast-grep: `execute*`/`query*`/`prepare*`/HTTP `send` calls inside `for`/`while`/`.for_each` bodies "
             "(suggest batching or hoisting `prepare_cached`); ingestion loops without `err_if_interrupted()`.",
         members=["c3:no-per-item-roundtrips-in-loops", "c5:sql-avoid-per-item-queries-in-loops",
                  "c4:interrupt-checks-in-long-loops", "c6:hoist-loop-invariant-work"]),
    dict(id="rs-record-cleanup", group="sql", tier=2,
         title="Tables filled from Remote Settings records carry `record_id` and are cleaned on deletion",
         how="Suggest-specific schema test: every table populated from an RS record has a `record_id` column and is "
             "covered by the record-deletion path; read/decrypt paths never `DELETE` user data.",
         members=["c4:suggest-tables-need-record-id-cleanup", "c7:no-data-deletion-in-read-paths"]),
    dict(id="persisted-values-stable", group="sql", tier=0,
         title="Persisted enum discriminants and hashes are stable",
         how="`disallowed-types = [\"std::collections::hash_map::DefaultHasher\"]` (its output changes between Rust "
             "releases); fieldless enums stored in the DB or exported get explicit discriminants (custom check: "
             "`#[repr(u8)]` enum with `ToSql` impl but no `= n`).",
         members=["c5:uniffi-enum-explicit-discriminants", "c6:persisted-enum-discriminants-frozen",
                  "c4:persisted-hash-stability-and-banned-hashes"]),
    # ------------------------------------------------------------------ remote
    dict(id="tolerant-remote-deserialization", group="remote", tier=1,
         title="Types deserialized from server/Remote Settings data tolerate absent and unknown values",
         how="ast-grep over `Deserialize` types in RS/merino/sync crates: no `#[serde(deny_unknown_fields)]`; new "
             "non-`Option` fields need `#[serde(default)]`; `deserialize_with` on optional fields also needs `default`; "
             "enums need an `#[serde(other)] Unknown` variant; `#[serde(untagged)]` enums whose earlier variant "
             "shadows a later one.",
         members=["c7:remote-data-new-fields-tolerate-absence", "c7:remote-enums-need-safe-unknown-variant",
                  "c6:remote-settings-record-schema-compat", "c4:rs-attachments-no-deny-unknown-fields",
                  "c4:rs-optional-fields-are-option", "c4:serde-deserialize-with-needs-default",
                  "c3:remote-data-tolerant-ingestion", "f1:serde-untagged-shadowing", "c4:serde-consistent-rename-casing"]),
    dict(id="sync-payload-compat", group="remote", tier=2,
         title="Sync payloads round-trip unknown fields and tolerate nulls",
         how="Test-harness rule: each sync engine's payload type has a round-trip test with an unknown field, an "
             "unknown enum value and explicit `null`s (ADR-0008).",
         members=["f2:serde-sync-payload-robustness", "c4:sync-payload-backward-compat", "c2:sync-payload-roundtrip-unknown-fields"]),
    # ------------------------------------------------------------- concurrency
    dict(id="no-callbacks-or-io-under-lock", group="concurrency", tier=2,
         title="Never call foreign callbacks, do network I/O, or re-lock while holding a store lock",
         how="`clippy::significant_drop_in_scrutinee`/`significant_drop_tightening` (nursery) catch guards that live "
             "too long; a custom lint flags calls on `Arc<dyn …>` callback trait objects or viaduct requests while a "
             "`MutexGuard` from the same fn is alive, and nested `.lock()` of the same field.",
         members=["c7:foreign-callbacks-no-lock-and-clean-shutdown", "c5:no-foreign-callbacks-while-holding-store-mutex",
                  "c3:no-lock-across-callback-or-io", "c6:lock-scope-discipline", "c4:sqlite-connection-discipline",
                  "f1:no-reentrant-db-lock", "c4:lock-guard-apis-document-deadlocks", "c3:lock-guard-scope"]),
    dict(id="lock-once-per-operation", group="concurrency", tier=3,
         title="Acquire a lock once per operation, at the layer that owns the state",
         how="Review; heuristic: more than one `.lock()` on the same receiver in a fn, or `.lock()` inside a loop.",
         members=["c2:lock-once-per-operation", "c7:one-lock-acquisition-per-operation"]),
    dict(id="parking-lot-locks", group="concurrency", tier=0,
         title="Use `parking_lot` locks, not `std::sync::Mutex`/`RwLock`",
         how="`disallowed-types = [\"std::sync::Mutex\", \"std::sync::RwLock\"]` in clippy.toml.",
         members=["c2:use-parking-lot-locks", "c3:use-parking-lot-mutex"]),
    dict(id="no-blocking-io-in-constructors", group="concurrency", tier=3,
         title="No disk/network I/O in constructors of exported objects",
         how="Heuristic: `#[uniffi::constructor]`/`new()` of exported objects calling `open*`/`read*`/viaduct; "
             "otherwise review.",
         members=["c1:no-blocking-io-on-calling-thread", "c2:no-network-io-in-constructors"]),
    dict(id="arc-try-unwrap-clone", group="concurrency", tier=1,
         title="`Arc::try_unwrap(x.clone())` always fails",
         how="ast-grep: `Arc::try_unwrap($X.clone())`, `Arc::into_inner($X.clone())`.",
         members=["c5:arc-try-unwrap-on-clone-always-fails"]),
    dict(id="no-global-mutable-statics", group="concurrency", tier=1,
         title="No `static Mutex<…>` caches for state that belongs in the DB or a struct",
         how="ast-grep: `static $N: $T = …` / `LazyLock`/`Lazy` where the type contains `Mutex`/`RwLock`/`RefCell`.",
         members=["c4:no-global-mutable-statics"]),
    # ----------------------------------------------------------------- privacy
    dict(id="no-pii-in-logs", group="privacy", tier=1,
         title="No URLs, titles, usernames or record payloads in logs, errors or reports",
         how="ast-grep: log/tracing/`report_error!`/`format!`-into-error macros whose args are named or typed "
             "`url`/`Url`/`title`/`username`/`origin`/`*_record`/`payload`, or that use `{:?}` on sync record types. "
             "Require the `error_support` redaction helpers (e.g. `redact_url`).",
         members=["c7:no-pii-in-errors-logs-and-reports", "c1:no-pii-in-logs", "c6:sanitize-urls-before-logging-or-reporting",
                  "f1:no-debug-logging-of-user-payloads"]),
    dict(id="tracing-support-logging", group="privacy", tier=0,
         title="Log through `tracing_support`, not the `log` crate",
         how="`disallowed-macros = [\"log::trace\", \"log::debug\", \"log::info\", \"log::warn\", \"log::error\"]`.",
         members=["f1:tracing-support-only-logging"]),
    # -------------------------------------------------------------------- docs
    dict(id="changelog-required", group="docs", tier=1,
         title="Consumer-visible component changes need a CHANGELOG.md entry",
         how="CI diff check (warning, overridable with a `skip-changelog` label): PR changes `components/*/src`, a "
             "`.udl`, or Kotlin/Swift wrappers but not CHANGELOG.md.",
         members=["c1:changelog-entry-required", "c3:changelog-entry-required", "c6:changelog-entry-required",
                  "c5:changelog-entry-for-user-visible-changes"]),
    dict(id="changelog-format", group="docs", tier=1,
         title="CHANGELOG edits go in the in-progress section, under a component heading, with a PR link",
         how="Script: only lines above the first released `# v…` header may change; bullets under `### <Component>` "
             "headings; PR link of the form `([#NNNN](https://github.com/mozilla/application-services/pull/NNNN))`; "
             "release PRs must bump `version.txt` and the CHANGELOG header together.",
         members=["c2:changelog-entry-format", "c4:changelog-pr-link-format", "c6:changelog-entry-in-current-version-section",
                  "c7:changelog-structure", "c1:changelog-format-and-placement", "c1:release-version-bump-consistency",
                  "c3:changelog-entry-quality"]),
    dict(id="spellcheck", group="docs", tier=0,
         title="Spell-check identifiers, comments, docs, UDL and CHANGELOG",
         how="`typos` (crate-ci/typos) in CI with a repo `_typos.toml` allowlist (component names, `megazord`, `udl`…). "
             "Fast, low noise and autofixable.",
         members=["c2:spellcheck-and-inclusive-language", "c3:spellcheck-code-and-docs", "c7:spellcheck-code-and-docs",
                  "c5:spellcheck-code-comments-and-scripts", "c1:typos-check", "c4:typos-check",
                  "c6:spellcheck-identifiers-and-docs"]),
    dict(id="docs-in-sync", group="docs", tier=0,
         title="Doc comments reference things that exist (intra-doc links, stale names)",
         how="`cargo doc` with `-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links` in CI and "
             "intra-doc links (`[`Foo`]`) instead of plain backticks so renames break the build. Stale prose stays a "
             "review item.",
         members=["c3:doc-links-and-identifiers-resolve", "c7:doc-comments-match-code", "c4:keep-docs-and-comments-in-sync",
                  "c5:remove-stale-or-misleading-comments", "c6:stale-comments-and-docs", "c1:rustdoc-intra-doc-links",
                  "c6:rustdoc-links-valid"]),
    dict(id="doc-comment-format", group="docs", tier=0,
         title="Well-formed doc comments: `///` for items, `//!` for modules, fenced examples",
         how="`clippy::doc_markdown`, `clippy::empty_line_after_doc_comments`, "
             "`clippy::too_long_first_doc_paragraph`, `clippy::doc_lazy_continuation`, plus a regex for `//` "
             "immediately above `pub` items in UDL/proc-macro files.",
         members=["c5:doc-comment-formatting", "c6:doc-comment-formatting", "f2:rustdoc-block-continuity",
                  "c3:rustdoc-examples-fenced", "c5:doc-comments-use-triple-slash"]),
    dict(id="todo-needs-bug", group="docs", tier=1,
         title="TODO/FIXME/XXX must reference a bug",
         how="Regex on added lines: `(TODO|FIXME|XXX)` not followed by a Bugzilla/GitHub reference "
             "(`bug \\d+`, `bugzil.la/\\d+`, `#\\d+`, a URL). `clippy::todo` for `todo!()`.",
         members=["c2:todo-xxx-must-link-issue", "c1:todo-must-reference-issue", "c4:todo-needs-bug-link",
                  "c5:todo-and-followups-reference-a-bug"]),
    dict(id="docs-index-and-adr", group="docs", tier=1,
         title="New docs are linked from SUMMARY.md; ADRs have complete metadata; links are permalinks",
         how="Script: every `docs/**/*.md` appears in `docs/SUMMARY.md`; ADR front-matter has Status/Deciders/Date and a "
             "unique number; GitHub links with `#L` anchors must use a commit SHA; no `docs.google.com` links.",
         members=["c1:docs-linked-from-summary", "f2:docs-summary-integrity", "c2:docs-summary-links",
                  "c1:adr-metadata-complete", "c1:docs-permalinks-and-public-links",
                  "c1:document-build-requirements-and-env-vars"]),
    dict(id="license-header-and-whitespace", group="docs", tier=1,
         title="Every source file starts with the MPL-2.0 header and ends with a newline",
         how="Script (or port mozilla-central's `license` and `file-whitespace` mozlint checks) over "
             "`*.rs|*.kt|*.swift|*.py|*.sh|*.udl|*.gradle`; `.editorconfig` with `insert_final_newline = true`. "
             "Crates need `license = \"MPL-2.0\"` in Cargo.toml.",
         members=["c1:mpl-license-header", "c2:mpl-license-header", "c3:mpl-license-header", "c5:mpl-license-header-required",
                  "c6:mpl-license-header", "f2:license-metadata-check", "f1:port-mozilla-central-linters",
                  "c6:final-newline", "c1:final-newline-at-eof", "c2:editorconfig-whitespace"]),
    dict(id="explain-magic-values", group="docs", tier=3,
         title="Name and explain magic values and non-obvious intent",
         how="Review checklist. Partial heuristic: numeric literals other than 0/1/2 in non-test code outside `const` "
             "items (noisy; keep as warning on changed lines only).",
         members=["c5:explain-magic-values-and-non-obvious-intent", "c7:named-constants-for-magic-values",
                  "c4:no-magic-numbers", "c3:no-magic-numbers", "c2:reuse-constants-no-magic-strings",
                  "c6:comment-forced-resync"]),
    # ---------------------------------------------------------------- deadcode
    dict(id="expect-with-reason", group="deadcode", tier=0,
         title="No blanket `#[allow(dead_code|unused)]`; use `#[cfg]` or `#[expect(…, reason = …)]`",
         how="`clippy::allow_attributes` + `clippy::allow_attributes_without_reason`: every suppression becomes an "
             "`#[expect]` with a reason, so it fails once it stops being needed.",
         members=["c6:no-allow-dead-code", "c3:no-blanket-allow-attributes", "c1:no-blanket-allow-prefer-cfg",
                  "c2:no-unjustified-allow-attributes", "c7:no-dead-code-or-blanket-allows", "c4:suppressions-need-reason"]),
    dict(id="no-debug-leftovers", group="deadcode", tier=0,
         title="No `println!`/`eprintln!`/`dbg!` (or `printStackTrace`/`print`) in shipped code",
         how="`clippy::print_stdout`, `clippy::print_stderr`, `clippy::dbg_macro` on library crates (CLIs and "
             "examples opt out at crate level); detekt `PrintStackTrace`; SwiftLint `no_print` custom rule.",
         members=["c6:no-debug-leftovers", "c2:no-leftover-debug-code", "c1:no-debug-output-in-library-code",
                  "c7:no-leftover-debug-and-untracked-todos"]),
    dict(id="no-commented-out-code", group="deadcode", tier=1,
         title="Delete commented-out code",
         how="Regex on added lines: `//` followed by something that parses as Rust (`let `, `fn `, `;$`, `}$`, "
             "`.unwrap()`), outside doc comments.",
         members=["c1:no-commented-out-code", "c5:no-commented-out-code", "c3:no-commented-out-code-or-untracked-todo"]),
    dict(id="remove-dead-pub-code", group="deadcode", tier=1,
         title="Remove unused pub items, error variants and columns that rustc can't see",
         how="`unreachable_pub` (rustc) turns crate-internal `pub` into `pub(crate)` so `dead_code` can see it; "
             "`warnalyzer`-style workspace scan for pub items with no users; `clippy::used_underscore_binding`.",
         members=["c4:dead-code-and-stale-underscores", "c3:delete-dead-and-deprecated-internal-code",
                  "c1:remove-dead-code-and-unused-api", "c5:remove-dead-code-unused-columns-and-files",
                  "c6:remove-unused-pub-items", "c2:no-underscore-prefix-on-used-bindings"]),
    dict(id="no-unused-deps", group="deadcode", tier=0,
         title="No unused or misplaced dependencies",
         how="`cargo machete` (or `cargo shear`) in CI; test-only crates (`mockito`, `tempfile`, `env_logger`) must be in "
             "`[dev-dependencies]`.",
         members=["c6:no-unused-dependencies", "c7:no-unused-dependencies", "f2:no-unused-deps",
                  "c1:no-unused-or-duplicate-dependencies", "c2:remove-unneeded-dependencies",
                  "c4:test-only-deps-in-dev-dependencies"]),
    # ------------------------------------------------------------------ idioms
    dict(id="option-result-combinators", group="idioms", tier=0,
         title="Use Option/Result combinators and `let-else` instead of manual matches",
         how="pedantic subset: `manual_let_else`, `option_if_let_else` (nursery), `map_unwrap_or`, "
             "`unnecessary_wraps`, `match_same_arms`, `if_not_else`, `redundant_closure_for_method_calls`.",
         members=["c3:clippy-option-bool-match-idioms", "c7:idiomatic-control-flow", "c5:idiomatic-option-result-combinators",
                  "c4:option-result-combinator-idioms", "c1:prefer-idiomatic-result-combinators", "c6:manual-option-map",
                  "c6:use-let-else", "c6:unwrap-or-default", "c5:simplify-redundant-branches", "c4:merge-identical-match-arms",
                  "c5:no-needless-result-wrapping"]),
    dict(id="needless-clones", group="idioms", tier=0,
         title="Avoid needless clones and allocations; write `Arc::clone(&x)`",
         how="`clippy::redundant_clone` (nursery), `implicit_clone`, `str_to_string`, `inefficient_to_string`, "
             "`clone_on_ref_ptr` (makes `Arc::clone` explicit), `needless_pass_by_value`.",
         members=["c4:avoid-needless-clone-and-borrow", "c2:avoid-needless-clone-and-borrow-mismatch", "c7:avoid-needless-clones",
                  "c5:avoid-needless-clones-and-allocations", "c1:avoid-needless-clones-explicit-arc-clone",
                  "c6:avoid-unnecessary-clones", "c3:clippy-avoid-needless-clones-and-owned-compares",
                  "c4:str-to-owned-not-to-string", "c4:single-char-pattern"]),
    dict(id="borrowed-params", group="idioms", tier=0,
         title="Take `&str`/`&[T]`/`Option<&T>` instead of `String`/`&Vec<T>`/`&Option<T>`",
         how="`clippy::ptr_arg` (default), `clippy::ref_option`, `clippy::needless_pass_by_value`, "
             "`clippy::trivially_copy_pass_by_ref`.",
         members=["c5:borrow-params-slices-str-option-ref", "c7:borrowed-parameter-types", "c3:borrow-params-not-owned",
                  "c4:take-params-by-reference", "c7:option-ref-not-ref-option", "c6:asref-str-generic-bounds"]),
    dict(id="collection-idioms", group="idioms", tier=0,
         title="Use `collect`/`extend`/entry API; compile regexes once",
         how="`clippy::manual_extend`-style lints (`extend_with_drain`, `same_item_push`), `map_entry` (default), "
             "`needless_collect`, `explicit_iter_loop`, plus `clippy::regex_creation_in_loops`.",
         members=["c4:collection-building-idioms", "c5:idiomatic-iterators-and-matches", "c6:entry-api-or-default",
                  "c7:hashmap-entry-api", "c5:prefer-split-whitespace", "c5:compile-regex-and-evaluators-once",
                  "c6:anchored-regex-compiled-once"]),
    dict(id="pedantic-subset", group="idioms", tier=0,
         title="A small curated pedantic subset (derives, unit structs, `Default` use)",
         how="`derivable_impls`/`new_without_default` (default), `empty_structs_with_brackets`, "
             "`empty_enum_variants_with_brackets`, `default_trait_access`, `uninlined_format_args`, "
             "`semicolon_if_nothing_returned`.",
         members=["c2:clippy-idiom-lints", "c4:enable-selected-pedantic-idioms", "c6:pedantic-idiom-nits", "c6:derive-default",
                  "c5:derive-default-instead-of-manual-constructors", "c5:derive-standard-traits-on-plain-data",
                  "c5:empty-variants-and-structs-without-brackets", "c3:unit-struct-without-braces", "c6:unit-struct-no-constructor",
                  "c4:prefer-concrete-constructors-over-default", "c7:needless-mut-and-smart-pointers",
                  "c4:no-unnecessary-shared-ownership"]),
    dict(id="exhaustive-matches", group="idioms", tier=0,
         title="No wildcard arms on domain enums; destructure fully in merge/hash/eq impls",
         how="`clippy::wildcard_enum_match_arm` / `match_wildcard_for_single_variants` scoped to state machines and "
             "error mapping (`#![warn]` per module); custom check that manual `Hash`/`PartialEq`/`merge()` impls "
             "destructure `Self { a, b, .. }` without `..`.",
         members=["c5:exhaustive-matches-and-destructuring", "f1:exhaustive-state-machine-matches",
                  "c6:merge-functions-exhaustive", "c7:manual-hash-covers-cache-key", "c4:float-fields-total-order"]),
    dict(id="deterministic-iteration", group="idioms", tier=0,
         title="Don't let HashMap/HashSet order leak into results, storage or generated output",
         how="`clippy::iter_over_hash_type` (restriction) on crates that generate code or persist data (nimbus-fml, "
             "suggest, search); use `BTreeMap`/`IndexMap` there.",
         members=["f2:deterministic-iteration", "c5:deterministic-ordering"]),
    dict(id="rustfmt-config", group="idioms", tier=0,
         title="Let rustfmt own imports and layout (incl. inside macros)",
         how="`rustfmt.toml`: `group_imports = \"StdExternalCrate\"`, `imports_granularity = \"Module\"`, "
             "`format_macro_matchers = true` (nightly-only options need `cargo +nightly fmt` in CI); "
             "`clippy::wildcard_imports`; pre-push hook running `cargo fmt`.",
         members=["c7:rustfmt-import-grouping", "c7:no-glob-local-or-fully-qualified-imports", "c2:import-hygiene",
                  "c3:rustfmt-blind-spots", "c4:formatting-applied", "c6:fmt-and-clippy-before-review",
                  "c3:no-stray-blank-lines", "c7:source-item-ordering", "c1:unit-tests-use-crate-paths"]),
    dict(id="unsafe-and-nss", group="idioms", tier=0,
         title="Every `unsafe` block has a `// SAFETY:` comment; NSS allocations use RAII",
         how="`clippy::undocumented_unsafe_blocks`, `clippy::multiple_unsafe_ops_per_block`; ast-grep for raw NSS "
             "`*_Free`/`PORT_Free` calls outside `Drop` impls; Windows/ABI check of `nss_sys` signatures.",
         members=["c5:unsafe-blocks-require-safety-comments", "c4:unsafe-needs-safety-comment", "c6:nss-ffi-memory-raii",
                  "f1:nss-raw-alloc-raii-and-lsan", "f1:nss-ffi-abi-checks"]),
    dict(id="duplicated-code", group="idioms", tier=3,
         title="Don't copy-paste logic; reuse existing helpers",
         how="Optional `jscpd`/`similarity-rs` report on changed files (informational only).",
         members=["c3:dry-duplicated-logic", "c7:duplicated-code-detection", "c5:reuse-existing-helpers-avoid-duplication"]),
    # --------------------------------------------------------------------- api
    dict(id="minimal-visibility", group="api", tier=0,
         title="Use the narrowest visibility (`pub(crate)`/private)",
         how="rustc `unreachable_pub` lint (machine-applicable fix).",
         members=["c2:minimal-visibility", "c3:minimal-visibility", "c7:minimal-visibility", "c1:minimize-visibility",
                  "c5:prefer-pub-crate-for-internal-items", "c4:visibility-pub-only-for-exported"]),
    dict(id="no-bool-params", group="api", tier=0,
         title="Replace boolean flags/positional tuples with enums or named structs",
         how="`clippy::fn_params_excessive_bools`, `clippy::struct_excessive_bools` (pedantic); review for single bools.",
         members=["c4:boolean-trap-params", "c6:no-opaque-bool-or-tuple-apis", "c2:no-positional-bool-option-args",
                  "c3:prefer-enums-over-bool-flags"]),
    dict(id="url-hygiene", group="api", tier=1,
         title="Build URLs with `url::Url`, never by string concat; no hard-coded endpoints",
         how="ast-grep: `format!(\"{}/…\", base…)` or `+ \"/\"` on values named `*url*`/`*endpoint*`; "
             "`https://firefox.settings.services.mozilla.com` (and friends) outside the server constants; non-HTTPS "
             "URLs outside tests; `*_url` naming for URL-valued fields.",
         members=["c4:build-urls-with-url-crate", "c6:build-urls-with-url-crate", "c7:url-building-via-url-api",
                  "f1:url-construction-hygiene", "c5:rs-root-url-without-trailing-slash", "c5:rs-no-hardcoded-server-urls",
                  "c6:no-hardcoded-endpoints-or-user-agent", "c6:https-only-requests", "c3:url-valued-fields-suffix-url"]),
    dict(id="typed-time-and-units", group="api", tier=1,
         title="Typed durations, sizes and timestamps; don't substitute `now` for bad data",
         how="ast-grep: integer params/fields named `*_ms|*_secs|*_seconds|timeout|ttl|interval` (suggest `Duration` or "
             "a newtype); `unwrap_or(now)`/`unwrap_or_else(|| now())` on timestamp parsing; `std::mem::size_of` used "
             "as a payload size.",
         members=["c7:typed-units-for-durations-and-sizes", "c6:time-handling-typed-and-safe", "c7:deliberate-clock-handling",
                  "c2:payload-size-not-mem-size"]),
    dict(id="naming", group="api", tier=1,
         title="Descriptive identifiers: no cryptic abbreviations, follow Rust API naming",
         how="`clippy::min_ident_chars` + `disallowed-names` (`nm`, `og`, `th`, `ta`, `tmp`, `foo`), "
             "`clippy::module_name_repetitions` (optional), `wrong_self_convention`, `should_implement_trait` (default).",
         members=["c4:no-cryptic-abbreviations", "c3:naming-no-abbreviations-or-placeholders",
                  "c5:no-abbreviations-and-rust-acronym-casing", "c1:rust-api-naming-conventions", "c3:naming-verb-conventions",
                  "c4:from-impl-param-naming", "c5:rename-identifiers-with-their-type", "c7:copy-paste-identifier-mismatch",
                  "c6:serialized-string-key-conventions"]),
    # -------------------------------------------------------------------- deps
    dict(id="component-wiring", group="deps", tier=1,
         title="New/renamed components are registered everywhere (megazords, uniffi.toml, Swift, Gradle, CI)",
         how="Script: derive the component list from `megazords/*/Cargo.toml` + `src/lib.rs` and check each one appears "
             "in `megazords/ios-rust` headers/modulemap, `uniffi.toml` (with the expected Kotlin package / Swift module "
             "keys), `.buildconfig-android.yml`, the generated-bindings allowlist and CI matrices.",
         members=["f2:megazord-consistency", "c7:new-component-scaffolding-matches-template", "c3:new-component-wired-into-builds",
                  "c2:ios-megazord-and-xcode-registration", "c6:component-registrations-in-sync",
                  "c5:derive-component-lists-from-megazord", "f1:generated-bindings-allowlist"]),
    dict(id="version-sync", group="deps", tier=1,
         title="One UniFFI/Glean version everywhere; toolchain versions in sync; no accidental submodule moves",
         how="Script: `cargo tree -d -i uniffi` empty; Glean version equal in Cargo.lock, `gradle/libs.versions.toml`, "
             "`Package.resolved` and the submodule; Android SDK/NDK and Rust MSRV equal across gradle, CI and docs; "
             "fail if `components/external/glean` changes without a matching Cargo.lock change.",
         members=["c1:single-uniffi-version", "c2:uniffi-dependency-hygiene", "c4:glean-uniffi-bump-companion-files",
                  "c2:cross-platform-dependency-version-sync", "c5:toolchain-versions-in-sync", "c3:submodule-pointer-unchanged"]),
    dict(id="compile-everything-on-pr", group="deps", tier=1,
         title="Every PR compiles all targets that a change can break (wrappers, examples, tests, taskgraph)",
         how="CI: `cargo check --workspace --all-targets` (catches examples/benches/orphaned tests); path-triggered "
             "Kotlin and Swift wrapper builds with warnings-as-errors against fresh bindings; `taskgraph` validated for "
             "every parameter set.",
         members=["f2:compile-wrappers-and-generated-code-on-pr", "f1:path-triggered-platform-ci",
                  "c6:build-all-workspace-members-incl-examples", "c7:all-test-crates-compile-in-ci", "c1:no-orphan-rust-files",
                  "f2:swift-warnings-as-errors", "c3:swift-respect-ios-deployment-target", "f2:taskgraph-validate-all-params",
                  "c6:dependency-minimum-versions-build"]),
    dict(id="cargo-feature-hygiene", group="deps", tier=1,
         title="Optional deps and the code that uses them sit behind the same feature; check every combination",
         how="`cargo hack check --each-feature` (or `--feature-powerset --depth 2`) for component crates; rustc's "
             "`unexpected_cfgs` (default since 1.80) for misspelled features; cfg-gate whole items, not statements.",
         members=["c2:cargo-feature-gating-consistency", "c3:cargo-feature-gated-deps", "c6:feature-combinations-built-in-ci",
                  "c7:cargo-feature-naming-and-docs", "c4:cfg-feature-names-valid", "c7:cfg-gate-whole-items",
                  "c1:megazord-no-duplicate-native-libs"]),
    dict(id="dependency-vetting", group="deps", tier=1,
         title="New/bumped crates must be vetted and compatible with mozilla-central",
         how="CI: new entries in Cargo.lock need a `cargo vet` audit (or mozilla-central's vet imports) and must satisfy "
             "the versions m-c vendors (compare against m-c `Cargo.lock`); `cargo audit` ignores need a reason comment; "
             "make the dependency-summary check required.",
         members=["c6:dependency-changes-vetted-and-intentional", "c7:new-dependency-needs-vetting", "c5:new-dependency-vetting",
                  "c3:new-dependency-registration", "f2:mc-vendoring-compat", "c1:mozilla-central-compatible-dependency-versions",
                  "c2:advisory-ignores-need-reason", "f2:required-dependency-check"]),
    dict(id="generated-files-untouched", group="deps", tier=1,
         title="Don't hand-edit generated files or commit unrelated churn",
         how="CI: regenerate DEPENDENCIES.md/license XML and fail on diff (already exists — make it required); reject "
             "generated bindings, `*.xcodeproj` / `Package.resolved` churn and empty files unless the PR is labelled.",
         members=["c5:dependencies-md-is-generated", "c4:lockfile-and-dependency-summary-hygiene",
                  "c1:lockfile-change-requires-dependency-summary", "c1:no-generated-code-checked-in", "c1:no-unrelated-file-churn"]),
    dict(id="crate-manifest", group="deps", tier=1,
         title="New crate manifests: license, description, `exclude` platform dirs, no deps on `examples/`",
         how="Script over `cargo metadata`: `license`, `description`, `edition` set; component crates `exclude = "
             "[\"/android\", \"/ios\"]`; no component depends on an `examples/*` crate; new crates are in "
             "`default-members` where appropriate.",
         members=["c2:new-crate-manifest-checklist", "f1:crate-excludes-platform-dirs", "f1:no-component-depends-on-examples"]),
    dict(id="build-rs-hygiene", group="deps", tier=1,
         title="`build.rs` writes only to `OUT_DIR`, doesn't shell out, fails with actionable messages",
         how="ast-grep in `build.rs`: `std::process::Command`, writes to paths not derived from `OUT_DIR`, bare "
             "`unwrap()` on env vars.",
         members=["c1:build-script-hygiene"]),
    # ------------------------------------------------------------------- tests
    dict(id="tests-with-behaviour", group="tests", tier=1,
         title="New behaviour comes with tests in the same PR",
         how="CI signal (warning only): PR adds/changes non-test code in `components/*/src` but no `#[test]`, "
             "`tests/` or binding test changes. Pair with per-PR coverage diff (codecov is already wired).",
         members=["c3:tests-accompany-behavior-changes", "c4:tests-required-for-new-behaviour", "c6:tests-required-for-new-logic",
                  "c5:tests-must-run-and-cover-edge-cases"]),
    dict(id="meaningful-assertions", group="tests", tier=1,
         title="Tests must actually assert: no bare `matches!`, mockito mocks asserted, messages match",
         how="ast-grep in test code: `matches!(…);` as a statement, `mockito` `.expect(n)` without `.assert()`, "
             "`assert!(x.is_some())` where the value is then unwrapped, duplicated assertion messages within a test.",
         members=["c5:assertion-messages-match-the-test", "c7:no-vacuous-test-assertions", "c7:tests-assert-payload-values",
                  "f1:mockito-mocks-asserted"]),
    dict(id="test-isolation", group="tests", tier=1,
         title="Tests are isolated: no wall clock, shared global state, fixed DB paths or prod endpoints",
         how="ast-grep in tests: `SystemTime::now()`/`chrono::Utc::now()` used for expected values; global "
             "initializers without `#[serial]`; hard-coded file paths instead of `tempfile`; production hostnames.",
         members=["f2:tests-serialize-global-state", "c6:tests-use-isolated-storage", "f2:tests-no-wall-clock",
                  "c2:no-wall-clock-in-test-expectations", "f1:nss-init-in-tests", "c7:test-fixtures-no-production-endpoints"]),
    dict(id="test-structure", group="tests", tier=3,
         title="Focused, descriptively named tests; shared fixtures via helpers",
         how="Review.",
         members=["c4:focused-descriptively-named-tests", "c3:test-fixtures-via-helpers-and-snapshots", "c2:slow-test-budget"]),
    # ----------------------------------------------------------------- scripts
    dict(id="shell-strict-shellcheck", group="scripts", tier=0,
         title="Shell scripts use `set -euo pipefail` and stay portable (shellcheck alone misses both)",
         how="`automation/lint_bash_scripts.sh` already shellchecks every `*.sh` in CircleCI. Add what shellcheck "
             "doesn't cover: require `set -euo pipefail` (or `set -e`), flag `sed -i` (GNU vs BSD differ), check exec "
             "bits match shebangs, and run `shfmt -d`.",
         members=["c1:shell-scripts-strict-and-shellchecked", "c5:shell-scripts-strict-mode-and-shellcheck",
                  "f2:shell-strict-and-shellcheck", "c3:shell-strict-mode-and-quoting", "f1:shell-script-portability",
                  "f2:exec-bit-matches-shebang", "c3:scripts-check-prereqs-and-run-locally"]),
    dict(id="python-ruff", group="scripts", tier=0,
         title="Python automation is formatted and linted (ruff) and exits non-zero on failure",
         how="`ruff format --check` + `ruff check` over `automation/`, `taskcluster/`, `tools/`, `testing/`; a rule that "
             "`main()` results reach `sys.exit`.",
         members=["c2:lint-python-and-shell-scripts", "c3:python-format-and-lint", "c5:python-scripts-ruff",
                  "f2:python-propagate-failures", "c2:taskgraph-cron-target-tasks"]),
    dict(id="pin-ci-tools", group="scripts", tier=1,
         title="Pin CI tools, images and actions (no `:latest`, current major of actions)",
         how="Regex over CI config: `:latest`, unpinned `curl | sh` installs, `actions/*@v<N>` behind the current "
             "major (Dependabot for GitHub Actions).",
         members=["c3:pin-ci-tool-versions", "c4:pin-ci-tool-versions", "f2:pin-lint-tool-versions", "f1:github-actions-hygiene"]),
    # ------------------------------------------------------------------ mobile
    dict(id="no-force-unwrap-wrappers", group="mobile", tier=0,
         title="No `!!`/force unwraps/force casts in Kotlin & Swift wrappers (incl. FML-generated code)",
         how="detekt `UnsafeCallOnNullableType`, `UnsafeCast`; SwiftLint `force_unwrapping`, `force_cast`, `force_try` "
             "(SwiftLint isn't in CI today).",
         members=["c5:kotlin-swift-no-force-unwrap-or-silent-sentinels", "c1:swift-no-force-unwrap-or-cast",
                  "c3:swift-no-force-unwrap-or-cast", "c7:kotlin-no-unsafe-casts"]),
    dict(id="swift-access-control", group="mobile", tier=0,
         title="Swift: explicit access control for consumer-facing API",
         how="SwiftLint `explicit_acl`/`explicit_top_level_acl` on `components/*/ios`.",
         members=["c2:swift-access-control-and-hygiene", "c3:swift-public-access-control"]),
    dict(id="wrappers-in-sync", group="mobile", tier=2,
         title="Hand-written Kotlin/Swift wrappers stay in sync with the Rust API and don't redefine UDL types",
         how="Covered by the API snapshot (rule `uniffi-api-break-gate`) plus a check that wrapper files don't declare a "
             "type with the same name as a generated one.",
         members=["c6:kotlin-swift-wrappers-in-sync", "c3:kotlin-swift-wrappers-dont-redefine-udl-types"]),
    dict(id="kotlin-gradle-misc", group="mobile", tier=0,
         title="Kotlin/Gradle hygiene: injectable dispatchers, KDoc params, lazy task APIs, no lint baselines",
         how="detekt rules (`InjectDispatcher`, `UndocumentedPublicFunction`), Gradle `--warning-mode=fail` for eager "
             "task APIs, reject new `lint-baseline.xml`.",
         members=["c7:kotlin-injectable-dispatchers", "c7:kotlin-kdoc-param-names", "c3:kotlin-style-ktlint-detekt",
                  "c6:no-android-lint-baselines", "c5:gradle-lazy-task-configuration", "f1:kotlin-unconditional-recursion",
                  "f2:android-uri-hierarchical-guard"]),
    # ------------------------------------------------------------ nimbus-glean
    dict(id="glean-metrics-yaml", group="nimbus-glean", tier=1,
         title="Validate metrics.yaml: bugs, data reviews, notification emails, expiry",
         how="`glean_parser check` in CI plus repo rules: non-empty `bugs`/`data_reviews`, team alias in "
             "`notification_emails`, no `disabled: false`, warn on metrics expiring within 2 releases.",
         members=["c6:glean-disabled-false-is-noop", "c2:glean-metrics-metadata", "c7:glean-metrics-yaml-hygiene",
                  "c6:glean-new-metric-needs-data-review", "c1:metrics-yaml-validated", "f2:glean-metric-expiry-guard"]),
    dict(id="fml-hygiene", group="nimbus-glean", tier=1,
         title="FML: pinned remote imports, backward-compatible manifests, clean templates and fixtures",
         how="nimbus-fml validation: remote `imports`/`includes` must pin a tag/SHA; diffing a manifest against `main` "
             "fails on removed/renamed keys; Askama templates have no unused `let`/imports.",
         members=["c3:fml-askama-template-unused", "c3:fml-fixture-hygiene", "c1:fml-remote-imports-pinned",
                  "c6:nimbus-fml-manifest-backward-compat"]),
]

# Candidates intentionally left out of the catalogue (too domain-specific to lint,
# or process advice). They are still listed in the per-chunk JSON.
UNMAPPED_OK = {
    "c5:bound-recursion-on-user-input", "c5:rs-use-collection-timestamp-not-max-record",
    "c6:separate-refactor-and-formatting-from-functional-changes",
}

# What each rule finds in the code today (main @ 5335439, 2026-10-26). Clippy
# numbers are for library targets only (`cargo clippy --workspace --lib`), so
# tests, examples and benches are excluded. "checks.py" / "ast-grep" refer to
# the prototypes in prototype/.
TODAY = {
    "uniffi-api-break-gate": "a naive diff heuristic (removed export lines without a \"breaking\" CHANGELOG line) flags 136 of 156 API-touching PRs since 2025, which is too noisy; this needs a real API snapshot, not a grep",
    "uniffi-unique-exported-names": "0 duplicates among 303 exported names in the 24 iOS-megazord crates (checks.py); 11 collision-prone generic names (`ApiError`, `Headers`, `Request`, `Store`, …)",
    "uniffi-docstrings": "398 UDL declarations/methods without `///`, 87 proc-macro exported types/fns without `///` (checks.py); `missing_docs`: 4357 pub items",
    "uniffi-handle-error": "27 fns return `ApiResult` without `#[handle_error]` (checks.py)",
    "no-unwrap-in-component-code": "`unwrap_used` 262, `expect_used` 75, `panic` 30 (many are `.lock().unwrap()` on std mutexes)",
    "no-panicking-index-or-arith": "`indexing_slicing` 105, `cast_possible_truncation` 50, `cast_sign_loss` 34, `arithmetic_side_effects` 284 (too noisy workspace-wide)",
    "utf8-safe-truncation": "`string_slice` 38, `String::truncate` 1 (`disallowed-methods`)",
    "dont-swallow-errors": "`let_underscore_must_use` 9, `map_err_ignore` 38, `.ok()`-style discards 29 (ast-grep)",
    "error-report-labels": "2 duplicated `report_error!` labels, 2 not kebab-case (checks.py)",
    "transaction-must-commit": "4 (ast-grep) — all read-only transactions; the rule would ask for an explicit `rollback()`",
    "sqlite-footguns": "`INSERT OR REPLACE`/`REPLACE INTO` 44 (ast-grep) — mostly key/value `meta` tables where it is harmless; scope it to tables with FK children or DELETE triggers",
    "schema-constraints": "10 `REFERENCES` without `ON DELETE` (all places, checks.py)",
    "pragmas-centralized": "13 `PRAGMA`s outside init/upgrade fns (checks.py, includes some test fixtures)",
    "migration-code-frozen": "1 literal version in a migration test (checks.py)",
    "schema-change-needs-migration": "backtest: 5 of 31 schema-touching PRs since 2025 changed DDL without bumping `END_VERSION`; 2 of them (ads-client #7036, #7088) added indexes that existing caches never get",
    "persisted-values-stable": "`DefaultHasher` used in 4 places (clippy `disallowed_types`)",
    "parking-lot-locks": "`std::sync::Mutex` 40, `std::sync::RwLock` 7 (clippy `disallowed_types`)",
    "tracing-support-logging": "`log::*!` macros: 24 (17 in relay) (clippy `disallowed_macros`)",
    "no-pii-in-logs": "76 log/report calls mentioning url/title/username/record/payload (ast-grep heuristic, needs triage)",
    "tolerant-remote-deserialization": "`deny_unknown_fields` 15, all in nimbus-fml manifests where strictness is intended; 0 on remote data types",
    "todo-needs-bug": "122 TODO/FIXME/XXX without a bug reference (checks.py)",
    "license-header-and-whitespace": "36 files without the MPL header, 11 without a final newline (checks.py)",
    "no-commented-out-code": "52 lines (checks.py heuristic)",
    "expect-with-reason": "`allow_attributes_without_reason` 147",
    "no-debug-leftovers": "`dbg_macro` 0, `print_stderr` 0, `print_stdout` 83 (mostly CLI/codegen crates that should opt out)",
    "remove-dead-pub-code": "`unreachable_pub` 837",
    "minimal-visibility": "`unreachable_pub` 837 (autofixable)",
    "option-result-combinators": "`option_if_let_else` 100, `redundant_closure_for_method_calls` 89, `manual_let_else` 45, `unnecessary_wraps` 43, `match_same_arms` 31, `map_unwrap_or` 22",
    "needless-clones": "`str_to_string` 315, `needless_pass_by_value` 148, `implicit_clone` 35, `clone_on_ref_ptr` 29, `redundant_clone` 12",
    "borrowed-params": "`needless_pass_by_value` 148, `trivially_copy_pass_by_ref` 15, `ref_option` 13",
    "collection-idioms": "`explicit_iter_loop` 21, `needless_collect` 3",
    "pedantic-subset": "`uninlined_format_args` 244, `semicolon_if_nothing_returned` 98, `default_trait_access` 80, `empty_structs_with_brackets` 9",
    "exhaustive-matches": "`wildcard_enum_match_arm` 136, `match_wildcard_for_single_variants` 24",
    "deterministic-iteration": "`iter_over_hash_type` 32",
    "rustfmt-config": "`unused_qualifications` 292, `wildcard_imports` 128",
    "unsafe-and-nss": "`undocumented_unsafe_blocks` 68 (62 in nss)",
    "no-bool-params": "`fn_params_excessive_bools` 0, `struct_excessive_bools` 4",
    "no-callbacks-or-io-under-lock": "`significant_drop_tightening` 21, `significant_drop_in_scrutinee` 0",
    "no-global-mutable-statics": "12 `static` items holding a Mutex/RwLock/RefCell (ast-grep)",
    "arc-try-unwrap-clone": "0 (ast-grep)",
    "no-force-unwrap-wrappers": "3 `!!` in Kotlin wrappers (checks.py)",
    "shell-strict-shellcheck": "17 `*.sh` without `set -e` (checks.py)",
    "doc-comment-format": "`doc_markdown` 593",
    "naming": "`disallowed_names` 34 (`nm` 32, `th` 2)",
    "docs-index-and-adr": "ADR-0006, ADR-0008 and ADR-0009 are not linked from `docs/SUMMARY.md`, so mdbook never publishes them",
    "changelog-required": "backtest: 381 of 603 consumer-visible PRs since 2025 (63%) had no CHANGELOG.md change, so a hard gate is too noisy; make it a warning with an opt-out label",
    "tests-with-behaviour": "backtest: 95 of 359 PRs with ≥20 new src lines (26%) touched no tests",
}
