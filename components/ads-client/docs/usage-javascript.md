# Mozilla Ads Client — JavaScript Usage Guide

This guide covers JavaScript-specific examples for using the Mozilla Ads Client (MAC) via UniFFI bindings. For the full API reference and type documentation, see [usage.md](./usage.md).

---

## Creating a Client

```javascript
const client = MozAdsClientBuilder()
    .environment(MozAdsEnvironment.Prod)
    .cacheConfig(cache)
    .telemetry(telemetry)
    .build();
```

---

## Implementing Telemetry

```javascript
class AdsClientTelemetry {
    recordBuildCacheError(label, value) {
        // Bind to your telemetry system
    }

    recordClientError(label, value) {
        // Bind to your telemetry system
    }

    recordClientOperationTotal(label) {
        // Bind to your telemetry system
    }

    recordDeserializationError(label, value) {
        // Bind to your telemetry system
    }

    recordHttpCacheOutcome(label, value) {
        // Bind to your telemetry system
    }
}
```

---

## Configuring the Cache

```javascript
const cache = MozAdsCacheConfig({
    dbPath: "/tmp/ads_cache.sqlite",
    defaultCacheTtlSeconds: 600,   // 10 min
    maxSizeMib: 20                 // 20 MiB
});

const telemetry = new AdsClientTelemetry();

const client = MozAdsClientBuilder()
    .environment(MozAdsEnvironment.Prod)
    .cacheConfig(cache)
    .telemetry(telemetry)
    .build();
```

---

## Per-Request Cache Policy Override

```javascript
// Always fetch from network but only cache for 60 seconds
const options = MozAdsRequestOptions({
    cachePolicy: MozAdsRequestCachePolicy({ mode: MozAdsCacheMode.NetworkFirst, ttlSeconds: 60 })
});

// Use it when requesting ads
const placements = client.requestImageAds(configs, options);
```
