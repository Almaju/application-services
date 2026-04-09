# Mozilla Ads Client — Kotlin Usage Guide

This guide covers Kotlin-specific examples for using the Mozilla Ads Client (MAC) via UniFFI bindings. For the full API reference and type documentation, see [usage.md](./usage.md).

---

## Creating a Client

```kotlin
val client = MozAdsClientBuilder()
    .environment(MozAdsEnvironment.PROD)
    .cacheConfig(cache)
    .telemetry(telemetry)
    .build()
```

---

## Implementing Telemetry

```kotlin
import mozilla.appservices.adsclient.MozAdsTelemetry
import org.mozilla.appservices.ads_client.GleanMetrics.AdsClient

class AdsClientTelemetry : MozAdsTelemetry {
    override fun recordBuildCacheError(label: String, value: String) {
        AdsClient.buildCacheError[label].set(value)
    }

    override fun recordClientError(label: String, value: String) {
        AdsClient.clientError[label].set(value)
    }

    override fun recordClientOperationTotal(label: String) {
        AdsClient.clientOperationTotal[label].add()
    }

    override fun recordDeserializationError(label: String, value: String) {
        AdsClient.deserializationError[label].set(value)
    }

    override fun recordHttpCacheOutcome(label: String, value: String) {
        AdsClient.httpCacheOutcome[label].set(value)
    }
}
```

---

## Configuring the Cache

```kotlin
val cache = MozAdsCacheConfig(
    dbPath = "/tmp/ads_cache.sqlite",
    defaultCacheTtlSeconds = 600L,   // 10 min
    maxSizeMib = 20L                 // 20 MiB
)

val telemetry = AdsClientTelemetry()

val client = MozAdsClientBuilder()
    .environment(MozAdsEnvironment.PROD)
    .cacheConfig(cache)
    .telemetry(telemetry)
    .build()
```

---

## Per-Request Cache Policy Override

```kotlin
// Always fetch from network but only cache for 60 seconds
val options = MozAdsRequestOptions(
    cachePolicy = MozAdsRequestCachePolicy(mode = MozAdsCacheMode.NETWORK_FIRST, ttlSeconds = 60L)
)

// Use it when requesting ads
val placements = client.requestImageAds(configs, options = options)
```
