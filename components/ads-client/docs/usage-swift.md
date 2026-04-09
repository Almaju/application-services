# Mozilla Ads Client — Swift Usage Guide

This guide covers Swift-specific examples for using the Mozilla Ads Client (MAC) via UniFFI bindings. For the full API reference and type documentation, see [usage.md](./usage.md).

---

## Creating a Client

```swift
let client = MozAdsClientBuilder()
    .environment(environment: .prod)
    .cacheConfig(cacheConfig: cache)
    .telemetry(telemetry: telemetry)
    .build()
```

---

## Implementing Telemetry

```swift
import MozillaRustComponents
import Glean

public final class AdsClientTelemetry: MozAdsTelemetry {
    public func recordBuildCacheError(label: String, value: String) {
        AdsClientMetrics.buildCacheError[label].set(value)
    }

    public func recordClientError(label: String, value: String) {
        AdsClientMetrics.clientError[label].set(value)
    }

    public func recordClientOperationTotal(label: String) {
        AdsClientMetrics.clientOperationTotal[label].add()
    }

    public func recordDeserializationError(label: String, value: String) {
        AdsClientMetrics.deserializationError[label].set(value)
    }

    public func recordHttpCacheOutcome(label: String, value: String) {
        AdsClientMetrics.httpCacheOutcome[label].set(value)
    }
}
```

---

## Configuring the Cache

```swift
let cache = MozAdsCacheConfig(
    dbPath: "/tmp/ads_cache.sqlite",
    defaultCacheTtlSeconds: 600,   // 10 min
    maxSizeMib: 20                 // 20 MiB
)

let telemetry = AdsClientTelemetry()

let client = MozAdsClientBuilder()
    .environment(environment: .prod)
    .cacheConfig(cacheConfig: cache)
    .telemetry(telemetry: telemetry)
    .build()
```

---

## Per-Request Cache Policy Override

```swift
// Always fetch from network but only cache for 60 seconds
let options = MozAdsRequestOptions(
    cachePolicy: MozAdsRequestCachePolicy(mode: .networkFirst, ttlSeconds: 60)
)

// Use it when requesting ads
let placements = client.requestImageAds(configs, options: options)
```
