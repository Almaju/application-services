# Telemetry

The metrics are in `metrics.yaml`. The client hands them to the `MozAdsTelemetrySink` the app passes to `MozAdsClientBuilder::telemetry_sink()`, one method per metric; on Android, that is `AdsClientTelemetrySink`. The first metrics in `metrics.yaml` go to the deprecated `MozAdsTelemetry` callback until it is removed.

## Errors

`src/telemetry/errors.rs` sorts errors into two events. The rule: could this happen on a healthy device, with a working network and every party behaving correctly? Yes means `expected_error`, no means `unexpected_error`. Only these kinds, codes and details are recorded, never an error's message.

| `expected_error` kind | Meaning |
|---|---|
| `timeout` | The request timed out, or the server answered 408 |
| `offline` | The device is offline |
| `dns` | The host name did not resolve |
| `network_other` | Any other network failure (all of them on iOS today) |
| `ohttp_relay` | The OHTTP relay failed |
| `http_server` | The server answered 5xx |
| `http_throttled` | The server answered 429 |
| `callback_gone` | A callback URL answered 404 or 410 |
| `shutdown` | The database was closed for shutdown |
| `storage_full` | The disk is full or failed |

| `unexpected_error` kind | Meaning |
|---|---|
| `invalid_response` | A response does not deserialize (`detail`: `syntax`, `data`, `eof` or `io`) |
| `invalid_ad_item` | An ad does not deserialize and is skipped (`detail`: the field path) |
| `http_bad_request` | The server answered 400 |
| `http_unexpected_status` | The server answered any other unsuccessful status |
| `invalid_request_config` | The caller passed duplicate or no placements |
| `invalid_callback_url` | The caller passed a callback URL that does not parse |
| `invalid_url` | A URL does not parse |
| `cache_config` | The cache or store configuration is invalid |
| `misconfigured` | The network stack is not set up as the client needs |
| `ohttp_config` | The OHTTP config could not be fetched |
| `ohttp_protocol` | The OHTTP response could not be decapsulated |
| `db_locked` | The database was busy or locked (we own every connection) |
| `db_corrupt` | The database is corrupt |
| `sql_error` | Any other SQLite error |
| `storage_downgrade` | The database was written by a newer version |
| `context_id` | The context ID could not be obtained |
