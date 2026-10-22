# Telemetry

The ads client records three kinds of telemetry, defined in `metrics.yaml` and `pings.yaml`. It hands them to the `MozAdsTelemetrySink` the app passes to `MozAdsClientBuilder::telemetry_sink()`, which records them in Glean; on Android, that is `AdsClientTelemetrySink`.

1. **Success metrics**, in the `metrics` ping: request, impression and click success rates, fill rate, cache hit rate, request latency (direct and OHTTP) and the number of context IDs issued.
2. **Expected errors**: errors a healthy client, server and network can still produce. Each one is an `ads_client.expected_error` event.
3. **Unexpected errors**: errors that point to a bug or a broken contract. Each distinct one is a record in `ads_client.error_reports`, sent in the `ads-client-errors` ping.

`src/telemetry/classify.rs` decides which errors are expected. The rule: could this happen on a healthy device, with a working network and every party behaving correctly? Yes means expected.

No error message is recorded except serde's, after `src/telemetry/scrub.rs` has removed its values. URLs, response bodies, paths, placement IDs and context IDs are never recorded.

The first five metrics in `metrics.yaml` (`build_cache_error`, `client_error`, `client_operation_total`, `deserialization_error` and `http_cache_outcome`) are the previous set, recorded through the `MozAdsTelemetry` callback until the new set has replaced them.

The values below are the only ones recorded. A test fails when the code records a value that is not listed here.

## Operations

The `operation` extra and the `op` field.

| Value | Operation |
|---|---|
| `clear_cache` | `clear_cache()` |
| `client_build` | Building the client. Errors here do not fail the build. |
| `record_click` | `record_click()` |
| `record_impression` | `record_impression()` |
| `report_ad` | `report_ad()` |
| `request_image_ads` | `request_image_ads()` |
| `request_spoc_ads` | `request_spoc_ads()` |
| `request_spoc_or_image_ads` | `request_spoc_or_image_ads()` |
| `request_tile_ads` | `request_tile_ads()` |

## Stages

The `stage` extra and field: the step the operation was at.

| Value | Step |
|---|---|
| `start` | Before the first step, e.g. parsing the callback URL. |
| `open` | Opening the cache or store database. |
| `context_id` | Getting or rotating the context ID. |
| `build_request` | Validating the placements and building the request. |
| `preflight` | The OHTTP preflight request. |
| `request` | The ad request, from the cache or the network. |
| `parse` | Deserializing the response. |
| `callback` | The click, impression or report request. |
| `cache` | Reading or writing the HTTP cache. |

## Expected error kinds

The `kind` extra of `ads_client.expected_error`.

| Value | Meaning |
|---|---|
| `timeout` | The request timed out, or the server answered 408. |
| `offline` | The device is offline. |
| `dns` | The host name did not resolve. |
| `connection` | The connection was refused, reset or interrupted. |
| `tls` | The TLS handshake or certificate check failed. |
| `proxy` | The proxy failed. |
| `cancelled` | The request was cancelled. |
| `network_other` | Any other transport failure, including every failure on iOS, whose backend does not report a cause. |
| `ohttp_config` | The OHTTP config could not be fetched. |
| `ohttp_relay` | The OHTTP relay failed. |
| `http_server` | The server answered 5xx. |
| `http_throttled` | The server answered 429. |
| `callback_gone` | A callback URL answered 404 or 410. |
| `shutdown` | The database was closed for shutdown. |
| `storage_busy` | The database was busy, locked or full, or the disk failed. |
| `storage_downgrade` | The database was written by a newer version. |

`code` holds the HTTP status, the network error code (`0x…` on Desktop, `gv:0x…` on Android) or the SQLite extended code.

## Unexpected error kinds

The `kind` field of the `ads_client.error_reports` records.

| Value | Meaning |
|---|---|
| `cache_config` | The cache or store configuration is invalid. |
| `context_id` | The context ID could not be obtained. |
| `db_corrupt` | The database is corrupt. |
| `db_recovery` | A corrupt database could not be deleted. |
| `http_bad_request` | The server answered 400. |
| `http_unexpected_status` | The server answered any other unsuccessful status. |
| `invalid_ad_item` | Ad items in a response did not deserialize and were skipped. One record per response. |
| `invalid_callback_url` | The caller passed a callback URL that does not parse. |
| `invalid_placement` | A placement in a response is not an array of ads, and was skipped. |
| `invalid_preflight` | The OHTTP preflight response did not deserialize. |
| `invalid_request_config` | The caller passed duplicate or no placements. |
| `invalid_response` | The ad response did not deserialize. |
| `invalid_url` | A URL did not parse. |
| `misconfigured` | The network stack is not set up as the client needs. |
| `ohttp_protocol` | The OHTTP response could not be decapsulated. |
| `sql_error` | Any other SQLite error. |

## Error records

Each record in `ads_client.error_reports` holds:

| Field | Content |
|---|---|
| `fp` | A fingerprint of the error, stable across clients and releases. |
| `kind`, `op`, `stage`, `fatal` | As above. `fatal` is set when the operation failed because of the error. |
| `chain` | The variant names of the error and its sources, outermost first. |
| `message` | A fixed description, or serde's message with its values removed. |
| `location` | The file and line that reported the error. |
| `steps` | The steps the operation went through, with the file and line of each. |
| `context` | Key and value pairs, listed below. |
| `count` | How many times the error happened since the previous ping. |

### Context keys

| Key | Value |
|---|---|
| `body_bytes` | The size of the HTTP response body. |
| `code` | The HTTP status or SQLite extended code. |
| `db_version` | The version of a database written by a newer client. |
| `invalid_items` | The number of ad items skipped in the response. |
| `io_kind` | The kind of I/O error. |
| `item_index` | The position of the first skipped item in its placement. |
| `serde_class` | `syntax`, `data`, `eof` or `io`. |
| `serde_path` | The path of the field that did not deserialize, e.g. `callbacks.click`. |
| `size_bytes` | The configured cache size. |
| `ttl_s` | The configured cache TTL. |

## Limits

- At most 50 `expected_error` events per operation and kind per process.
- At most 10 distinct unexpected errors per process. Later ones only increase `dropped`.
- At most 5 `ads-client-errors` pings per process.
- Nothing is sent while the client is being built: errors from that step go out at the end of the first operation.
