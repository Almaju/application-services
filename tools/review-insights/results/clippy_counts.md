# Lint hit counts on main (5335439)

Produced with `NSS_DIR=<dir with NSS/NSPR headers> CLIPPY_CONF_DIR=prototype cargo clippy --workspace --lib
--message-format=json -- -W clippy::<lint> …` (full flag list in `prototype/clippy-measure-flags.txt`). Library targets
only: tests, examples and benches are not counted. Deduplicated by lint + location.

| Lint | Hits | Crates | Top crates |
|---|---:|---:|---|
| `missing_docs` | 4357 | 62 | places 667, nss_sys 665, nimbus 457, autofill 296 |
| `unreachable_pub` | 837 | 31 | fxa_client 212, suggest 95, nimbus_fml 94, ads_client 83 |
| `clippy::use_self` | 799 | 37 | places 174, sync15 120, nimbus 81, nimbus_fml 62 |
| `clippy::missing_errors_doc` | 618 | 34 | places 116, nimbus 105, sync15 61, logins 57 |
| `clippy::doc_markdown` | 593 | 40 | sync15 67, places 65, fxa_client 65, suggest 48 |
| `clippy::must_use_candidate` | 389 | 38 | nimbus_fml 55, places 50, sync15 40, suggest 37 |
| `clippy::as_conversions` | 371 | 29 | places 150, nss_as 55, nimbus 32, autofill 18 |
| `clippy::str_to_string` | 315 | 27 | nimbus_fml 79, nimbus 28, merino 21, fxcontainers 21 |
| `unused_qualifications` | 292 | 26 | places 71, nimbus_fml 50, nimbus 28, logins 23 |
| `clippy::arithmetic_side_effects` | 284 | 33 | places 44, nimbus 29, sync15 27, nimbus_fml 25 |
| `clippy::unwrap_used` | 262 | 33 | nimbus 47, nimbus_fml 46, tabs 24, suggest 20 |
| `clippy::uninlined_format_args` | 244 | 25 | nimbus_fml 92, places 31, dump 22, autofill 19 |
| `clippy::exhaustive_enums` | 188 | 38 | places 28, nimbus 14, ads_client 13, sync15 11 |
| `clippy::needless_pass_by_value` | 148 | 28 | nimbus 26, fxa_client 13, nimbus_fml 12, relevancy 11 |
| `clippy::allow_attributes_without_reason` | 147 | 36 | nimbus_fml 21, fxa_client 19, nimbus 14, places 10 |
| `clippy::wildcard_enum_match_arm` | 136 | 25 | nimbus_fml 46, places 14, suggest 9, webext_storage 8 |
| `clippy::wildcard_imports` | 128 | 19 | places 28, autofill 20, nss_as 13, logins 12 |
| `clippy::allow_attributes` | 121 | 26 | nimbus_fml 21, fxa_client 19, nimbus 14, places 9 |
| `clippy::indexing_slicing` | 105 | 22 | places 20, nimbus_fml 12, nss_as 11, fxa_client 8 |
| `clippy::missing_panics_doc` | 102 | 24 | nimbus 34, sync15 9, places 9, sql_support 7 |
| `clippy::option_if_let_else` | 100 | 21 | places 20, nimbus 15, nimbus_fml 12, sync15 8 |
| `clippy::semicolon_if_nothing_returned` | 98 | 26 | fxa_client 23, nimbus_fml 13, places 8, suggest 7 |
| `clippy::redundant_closure_for_method_calls` | 89 | 19 | nimbus_fml 17, nimbus 16, suggest 12, autofill 7 |
| `clippy::print_stdout` | 83 | 10 | start_bindings 25, dump 17, nss_build_common 12, suggest 10 |
| `clippy::default_trait_access` | 80 | 12 | nimbus_fml 45, nimbus 11, search 6, firefox_versioning 4 |
| `clippy::expect_used` | 75 | 26 | places 18, nimbus_fml 14, nimbus 5, logins 4 |
| `clippy::undocumented_unsafe_blocks` | 68 | 5 | nss_as 62, megazord 2, cirrus 2, mozbuild 1 |
| `clippy::disallowed_types` | 51 | 10 | nimbus 21, tabs 9, nss_as 3, places 3 |
| `clippy::cast_possible_truncation` | 50 | 14 | places 17, nimbus 9, nimbus_fml 5, sync15 4 |
| `clippy::manual_let_else` | 45 | 18 | nimbus_fml 10, webext_storage 6, places 5, nimbus 5 |
| `clippy::unused_self` | 45 | 14 | nimbus_fml 27, nimbus 2, sync_manager 2, suggest 2 |
| `clippy::unnecessary_wraps` | 43 | 13 | nimbus_fml 25, places 4, autofill 3, merino 2 |
| `clippy::return_self_not_must_use` | 41 | 10 | places 11, sync15 7, text_table 6, error_support 5 |
| `clippy::map_err_ignore` | 38 | 15 | as_ohttp_client 10, viaduct 6, db_crypto 5, fxa_client 5 |
| `clippy::string_slice` | 38 | 12 | places 15, nimbus 4, nimbus_fml 4, suggest 3 |
| `clippy::cast_possible_wrap` | 36 | 9 | autofill 12, places 11, types 3, ads_client 3 |
| `clippy::implicit_clone` | 35 | 17 | nimbus 8, nimbus_fml 5, webext_storage 4, suggest 3 |
| `clippy::cast_sign_loss` | 34 | 13 | nimbus 10, places 8, sync15 3, logins 3 |
| `clippy::disallowed_names` | 34 | 2 | nimbus_fml 32, nimbus 2 |
| `clippy::iter_over_hash_type` | 32 | 14 | nimbus 8, suggest 4, sync15 3, nimbus_fml 3 |
| `clippy::match_same_arms` | 31 | 16 | places 6, nimbus_fml 5, sync15 3, sync_manager 3 |
| `clippy::panic` | 30 | 12 | places 6, sql_support 4, nss_as 3, viaduct 3 |
| `clippy::clone_on_ref_ptr` | 29 | 9 | nimbus 17, places 3, tracing_support 2, sync15 2 |
| `clippy::if_not_else` | 29 | 10 | nimbus 6, nimbus_fml 6, suggest 4, viaduct 3 |
| `clippy::disallowed_macros` | 24 | 4 | relay 17, crashtest 3, dump 3, fxa_client 1 |
| `clippy::match_wildcard_for_single_variants` | 24 | 7 | sync15 14, jwcrypto 4, as_ohttp_client 2, viaduct 1 |
| `clippy::map_unwrap_or` | 22 | 11 | places 4, ads_client 4, nimbus 3, push 3 |
| `clippy::explicit_iter_loop` | 21 | 13 | places 4, sync15 3, sync_manager 2, suggest 2 |
| `clippy::significant_drop_tightening` | 21 | 4 | nimbus 13, tabs 6, places 1, suggest 1 |
| `clippy::unnested_or_patterns` | 16 | 6 | firefox_versioning 5, nimbus_fml 5, sync15 2, nimbus 2 |
| `clippy::manual_string_new` | 15 | 9 | sync15 3, places 3, fxa_client 2, nimbus_fml 2 |
| `clippy::trivially_copy_pass_by_ref` | 15 | 7 | suggest 6, nss_as 3, ads_client 2, tracing_support 1 |
| `clippy::ref_option` | 13 | 7 | nimbus_fml 4, push 3, search 2, sync15 1 |
| `clippy::unimplemented` | 13 | 5 | nimbus 4, nimbus_fml 4, sync15 3, nss_as 1 |
| `clippy::redundant_clone` | 12 | 9 | suggest 4, tracing_support 1, remote_settings 1, context_id 1 |
| `clippy::let_underscore_untyped` | 11 | 7 | logins 3, context_id 2, ads_client 2, places 1 |
| `clippy::single_char_pattern` | 11 | 5 | start_bindings 5, tracing_support 2, embedded_uniffi_bindgen 2, error_support 1 |
| `let_underscore_drop` | 11 | 8 | nimbus 2, context_id 2, ads_client 2, places 1 |
| `clippy::empty_structs_with_brackets` | 9 | 5 | autofill 4, rc_crypto 2, nimbus 1, ads_client 1 |
| `clippy::let_underscore_must_use` | 9 | 6 | logins 3, ads_client 2, places 1, nimbus 1 |
| `clippy::manual_assert` | 6 | 4 | sql_support 2, places 2, nss_as 1, embedded_uniffi_bindgen 1 |
| `clippy::items_after_statements` | 5 | 3 | nss_build_common 3, remote_settings 1, webext_storage 1 |
| `clippy::struct_excessive_bools` | 4 | 4 | nimbus 1, ads_client 1, relay 1, nimbus_fml 1 |
| `clippy::needless_collect` | 3 | 2 | fxa_client 2, nimbus_fml 1 |
| `clippy::disallowed_methods` | 1 | 1 | ads_client 1 |
| `clippy::mem_forget` | 1 | 1 | places 1 |
| `clippy::dbg_macro` | 0 | 0 |  |
| `clippy::empty_enum_variants_with_brackets` | 0 | 0 |  |
| `clippy::float_cmp` | 0 | 0 |  |
| `clippy::fn_params_excessive_bools` | 0 | 0 |  |
| `clippy::inefficient_to_string` | 0 | 0 |  |
| `clippy::large_types_passed_by_value` | 0 | 0 |  |
| `clippy::lossy_float_literal` | 0 | 0 |  |
| `clippy::mutex_atomic` | 0 | 0 |  |
| `clippy::mutex_integer` | 0 | 0 |  |
| `clippy::print_stderr` | 0 | 0 |  |
| `clippy::rc_mutex` | 0 | 0 |  |
| `clippy::significant_drop_in_scrutinee` | 0 | 0 |  |
| `clippy::todo` | 0 | 0 |  |
| `clippy::unused_async` | 0 | 0 |  |
