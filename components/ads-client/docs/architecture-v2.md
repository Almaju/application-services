# Ads Client — current architecture and the v2 direction

**Status:** discussion document, for team alignment. Nothing here is committed to yet.
**Scope:** services, components and boundaries. No code, no type signatures, no schemas.

---

## The one-sentence version

Today the ads client is a **stateless HTTP SDK**: the consumer asks a question, we make a
request, we hand back a response. V2 makes it a **stateful ads client**: the consumer
*dispatches intent* and *queries a local read model*, and everything in between —
fetching, renewing, capping, selecting, reporting — is the SDK's problem.

The consumer experience we are aiming for is Sentry-shaped: **start it once, query it when
you need an ad, and stop thinking about it.** No cache tuning, no error handling, no
deciding when an impression happened.

| | Today | V2 |
| --- | --- | --- |
| Shape | request → response | dispatch → query |
| State | none survives a call | ads store, cap counters, outbox |
| Threading | caller's thread | one worker thread inside the SDK |
| Failure | thrown across the FFI | telemetry; `query()` returns nothing |
| Logic | HTTP, serialization, callback URLs | all ads business logic |

---

## 1. Current state

### 1.1 Component view

```mermaid
%%{init: {"theme":"base","themeVariables":{"fontSize":"14px","lineColor":"#5A6C69","textColor":"#16211F","clusterBkg":"#EFF3F1","clusterBorder":"#C3D0CC","edgeLabelBackground":"#F8FAF9"}}}%%
flowchart TB
  subgraph consumer["Consumer app — Firefox Android / iOS / Desktop"]
    direction LR
    cfg["Config and lifecycle<br/>db path · TTL · cache size · threading"]
    ui["Surface UI<br/>New Tab · Home · Top Sites"]
    imp["Impression decision<br/>app-owned viewability logic"]
  end

  subgraph ffi["FFI boundary — UniFFI, inbound"]
    direction LR
    builder["MozAdsClientBuilder<br/>environment · cacheConfig · rotationDays"]
    facade["MozAdsClient<br/>requestImage / Spoc / TileAds<br/>recordImpression · recordClick · reportAd · clearCache"]
  end

  subgraph rust["ads-client — Rust component"]
    direction TB
    orch["AdsClient<br/>build request · map errors · emit events"]
    ctxid["ContextIDComponent<br/>rotating context_id"]
    marsc["MARSClient<br/>fetch_ads · callback GETs"]
    subgraph cache["HttpCache — generic, request-keyed"]
      direction LR
      policy["CachePolicy<br/>CacheFirst / NetworkFirst + TTL"]
      hash["RequestHash<br/>hash of url + placements"]
      store["HttpCacheStore<br/>SQLite WAL · opaque response blobs"]
    end
    viaduct["viaduct client — platform HTTP stack"]
  end

  subgraph net["Mozilla services"]
    direction LR
    mars["MARS<br/>POST /v1/ads"]
    cbs["Callback endpoints<br/>impression · click · report"]
  end

  subgraph telem["FFI boundary — outbound telemetry"]
    direction LR
    telcb["MozAdsTelemetry foreign callback → Glean in the app"]
  end

  cfg --> builder
  builder -->|"build()"| facade
  ui -->|"requestXAds(placements) — blocks the caller"| facade
  imp -->|"recordImpression(url)"| facade
  facade -->|"FFI types to domain types"| orch
  orch -->|"context_id"| ctxid
  orch -->|"AdRequest + CachePolicy"| marsc
  marsc -->|"send_with_policy"| policy
  policy --> hash
  hash -->|"lookup / store blob"| store
  store -->|"on miss"| viaduct
  marsc -->|"callback GET"| viaduct
  viaduct --> mars
  viaduct --> cbs
  orch -.->|"typed events"| telcb
  store -.->|"hit / miss / store_failed"| telcb

  classDef app fill:#F3EDE4,stroke:#8A7A5C,color:#2B2418
  classDef bound fill:#E5ECF2,stroke:#5A7C93,color:#12242E
  classDef core fill:#FFFFFF,stroke:#4A4A52,color:#1A1A20
  classDef ext fill:#EFE6EC,stroke:#8A6A80,color:#2B1A26
  class ui,imp,cfg app
  class builder,facade,telcb bound
  class orch,ctxid,marsc,policy,hash,store,viaduct core
  class mars,cbs ext
```

Three boundaries, one direction of travel. Note what the cache actually is: a **generic
HTTP response cache** keyed on a hash of *(url, placements)*, storing opaque response
blobs. It cannot reason about a single ad — only replay a whole response.

### 1.2 What happens on one request

```mermaid
%%{init: {"theme":"base","themeVariables":{"fontSize":"14px","lineColor":"#5A6C69","textColor":"#16211F","actorBkg":"#E5ECF2","actorBorder":"#5A7C93","actorTextColor":"#12242E","noteBkgColor":"#F3EDE4","noteBorderColor":"#8A7A5C","noteTextColor":"#2B2418","signalColor":"#3B4B48","signalTextColor":"#16211F","labelBoxBkgColor":"#EFF3F1","labelBoxBorderColor":"#C3D0CC","labelTextColor":"#16211F","sequenceNumberColor":"#FFFFFF"}}}%%
sequenceDiagram
  autonumber
  participant UI as Surface UI
  participant FFI as MozAdsClient
  participant AC as AdsClient
  participant CID as ContextID
  participant HC as HttpCache
  participant V as viaduct
  participant M as MARS

  UI->>FFI: requestTileAds(placements, options)
  Note over UI,FFI: blocking — the consumer owns the thread
  FFI->>AC: domain types + CachePolicy
  AC->>CID: context_id (rotating)
  AC->>HC: send_with_policy(AdRequest, policy)
  HC->>HC: delete expired rows
  alt CacheFirst and hash present
    HC-->>AC: cached response bytes
  else miss, or NetworkFirst
    HC->>V: POST /v1/ads
    V->>M: request
    M-->>V: ad payload
    V-->>HC: response
    HC->>HC: store blob under request hash
  end
  AC->>AC: parse, attach hash + placement to callback URLs
  AC-->>FFI: placement to ad
  FFI-->>UI: map of ads, or throws
  Note over UI: the UI decides what "visible" means
  UI->>FFI: recordImpression(callback url)
  FFI->>V: GET impression callback
```

The whole component lives inside one blocking call. **Nothing survives the return:** not
what was served, not what was seen, not what caps were consumed — so nothing can inform
the next request.

### 1.3 Who owns what today

| Concern | Owner |
| --- | --- |
| When to request ads | Consumer |
| Threading, not blocking the UI | Consumer |
| Retry, backoff, offline behaviour | Nobody |
| Error handling and fallback UI | Consumer |
| What counts as an impression | Consumer |
| Frequency capping, rotation, pacing | Nobody — `caps` arrive on spocs and are unused |
| Cache lifetime, size, DB path | Consumer, via the builder |
| Which ad shows where | MARS, per request |
| HTTP, serialization, callback URLs | **SDK** |

The SDK owns the bottom row and nothing above it. That gap is what v2 closes, and it is
also why every surface currently reimplements the same ads logic slightly differently.

### 1.4 Four properties worth naming before we change them

- **The cache is an HTTP cache, not an ads cache.** Key: request hash. Value: response bytes.
- **`context_id` is deliberately excluded from the cache key**, so identity rotation does not
  invalidate cached responses. Correct for a response cache; an open question once we store
  ads individually.
- **Everything is synchronous.** No thread, no queue, no scheduler. Every call returns or
  throws, on the caller's thread.
- **Callbacks are fire-and-forget.** An impression is a blocking GET with no retry and no
  durability. If it fails, it is lost.

---

## 2. The change of shape

This is not "add a better cache". One request/response path splits into an independent
**write path** and **read path**, joined by a store that a background worker keeps warm.

```mermaid
%%{init: {"theme":"base","themeVariables":{"fontSize":"14px","lineColor":"#5A6C69","textColor":"#16211F","clusterBkg":"#EFF3F1","clusterBorder":"#C3D0CC","edgeLabelBackground":"#F8FAF9"}}}%%
flowchart LR
  subgraph now["Today — request / response"]
    direction TB
    c1["Consumer"] -->|"requestAds() — blocks"| h1["HTTP cache<br/>opaque blobs, keyed by request hash"]
    h1 -->|"miss"| n1["MARS"]
    n1 --> h1
    h1 -->|"response, or throws"| c1
  end

  subgraph next["V2 — dispatch / query"]
    direction TB
    c2["Consumer"] -->|"dispatch(intent) — returns at once"| q2["Command queue<br/>durable · ordered"]
    q2 --> w2["Background worker<br/>drains queue · renews expiring · applies policy"]
    w2 -->|"fetch"| n2["MARS"]
    n2 --> w2
    w2 -->|"project"| s2["Ads store<br/>one row per ad, with state"]
    s2 -->|"query() — local, non-blocking, never throws"| c2
  end

  h1 -.->|"replaced by"| s2
  c1 -.->|"becomes"| q2

  classDef old fill:#F5EAE1,stroke:#A0562F,color:#3A1F10
  classDef new fill:#DDEBE9,stroke:#2C6B67,color:#0D2B29
  class c1,h1,n1 old
  class c2,q2,w2,s2,n2 new
```

The store holds **ads**, not **responses**. Once an ad is a row with its own lifecycle —
fetched at, expires at, times shown, cap key, placement, block key — renewal, capping,
rotation and history-informed selection all become queries over local state instead of
new round trips.

---

## 3. V2 target architecture

### 3.1 The map

```mermaid
%%{init: {"theme":"base","themeVariables":{"fontSize":"14px","lineColor":"#5A6C69","textColor":"#16211F","clusterBkg":"#EFF3F1","clusterBorder":"#C3D0CC","edgeLabelBackground":"#F8FAF9"}}}%%
flowchart TB
  app["Consumer app<br/>renders ads · reports raw view events · dispatches lifecycle"]
  api["Ads client API — FFI boundary<br/>start · dispatch · query · observer · telemetry"]
  wr["Write side<br/>command API · durable queue · background worker · scheduler"]
  rdq["Read side entry<br/>selection and ranking"]
  dm["Domain policy<br/>placement registry · renewal · frequency capping · experiments · privacy gate"]
  store["Ads store — read model<br/>ad rows · placement index · cap counters · interaction outbox"]
  eg["Egress<br/>MARS client · callback dispatcher · context id · viaduct"]
  net["Mozilla services<br/>MARS · callback endpoints"]

  app -->|"dispatch(intent)"| api
  app -->|"query(placement)"| api
  api -->|"commands"| wr
  api -->|"reads"| rdq
  wr -->|"consults"| dm
  rdq -->|"consults"| dm
  wr -->|"fetch plan"| eg
  dm -->|"reads and writes"| store
  store -->|"drains outbox through"| eg
  eg -->|"projects responses into"| store
  eg --> net
  store -.->|"observer: placement filled"| api

  classDef app fill:#F3EDE4,stroke:#8A7A5C,color:#2B2418
  classDef bound fill:#E5ECF2,stroke:#5A7C93,color:#12242E
  classDef wrc fill:#E9E5F1,stroke:#6B5B93,color:#1E1830
  classDef dm fill:#FFFFFF,stroke:#4A4A52,color:#1A1A20
  classDef rd fill:#DDEBE9,stroke:#2C6B67,color:#0D2B29
  classDef eg fill:#F5EAE1,stroke:#A0562F,color:#3A1F10
  classDef ext fill:#EFE6EC,stroke:#8A6A80,color:#2B1A26
  class app app
  class api bound
  class wr,rdq wrc
  class dm dm
  class store rd
  class eg eg
  class net ext
```

The three paths through this map are drawn separately below, because the whole point of
the change is that they are independent.

### 3.2 Write path — dispatch

```mermaid
%%{init: {"theme":"base","themeVariables":{"fontSize":"14px","lineColor":"#5A6C69","textColor":"#16211F","clusterBkg":"#EFF3F1","clusterBorder":"#C3D0CC","edgeLabelBackground":"#F8FAF9"}}}%%
flowchart TB
  subgraph src["Sources of intent"]
    direction LR
    surf["Surface<br/>will need this placement soon"]
    life["App lifecycle<br/>foreground · connectivity"]
    ticks["Scheduler ticks<br/>renew · prefetch · retry · GC"]
  end

  disp["dispatch(command)<br/>FFI — returns immediately"]
  cmdapi["Command API<br/>validate · dedupe · stamp"]
  queue["Command queue<br/>durable · ordered · survives restart"]
  worker["Background worker<br/>the only thread that does network"]

  subgraph pol["Policy consulted while planning"]
    direction LR
    registry["Placement registry<br/>what exists · shape · count"]
    renew["Renewal policy<br/>TTL · expiry horizon · refresh-ahead"]
    exp["Experiment gate<br/>Nimbus variants to parameters"]
  end

  plan["Fetch plan<br/>which placements · how many · which variant"]
  marsc["MARS client<br/>context id · viaduct"]
  mars["MARS<br/>POST /v1/ads"]
  proj["Projection<br/>response to individual ad rows"]
  ads["Ads store<br/>ad rows + placement index"]
  obs["Observer callback<br/>placement filled — UI may re-render"]

  surf --> disp
  life --> disp
  disp -->|"enqueue"| cmdapi
  cmdapi --> queue
  queue -->|"drained by"| worker
  ticks -->|"wake"| worker
  worker --> registry
  worker --> renew
  worker --> exp
  registry --> plan
  renew --> plan
  exp --> plan
  plan --> marsc
  marsc --> mars
  mars -->|"ad payload"| proj
  proj -->|"write"| ads
  ads -.-> obs

  classDef app fill:#F3EDE4,stroke:#8A7A5C,color:#2B2418
  classDef wrc fill:#E9E5F1,stroke:#6B5B93,color:#1E1830
  classDef dm fill:#FFFFFF,stroke:#4A4A52,color:#1A1A20
  classDef rd fill:#DDEBE9,stroke:#2C6B67,color:#0D2B29
  classDef eg fill:#F5EAE1,stroke:#A0562F,color:#3A1F10
  classDef bound fill:#E5ECF2,stroke:#5A7C93,color:#12242E
  class surf,life app
  class disp,obs bound
  class cmdapi,queue,worker,ticks,plan wrc
  class registry,renew,exp dm
  class marsc,mars eg
  class proj,ads rd
```

Two things enter this path: consumer intent, and the worker's own schedule. They meet at
the same queue, so a renewal and a prefetch are handled by identical machinery.

### 3.3 Read path — query

```mermaid
%%{init: {"theme":"base","themeVariables":{"fontSize":"14px","lineColor":"#5A6C69","textColor":"#16211F","clusterBkg":"#EFF3F1","clusterBorder":"#C3D0CC","edgeLabelBackground":"#F8FAF9"}}}%%
flowchart TB
  surf["Surface UI<br/>about to render a slot"]
  query["query(placement)<br/>FFI — synchronous, local only"]

  gather["Gather candidates<br/>eligible ads for this placement"]
  filter["Apply caps<br/>drop what is exhausted or expired"]
  rank["Rank<br/>priority · variant · topic affinity"]
  out["0..n ads<br/>no network · no blocking · never throws"]
  miss["Nothing eligible<br/>not an error — enqueues a prefetch command"]

  subgraph store["Ads store — read model"]
    direction LR
    idx["Placement index<br/>placement to eligible ads"]
    rows["Ad rows<br/>fetched at · expires at · times shown · cap key"]
    capstore["Cap counters<br/>impressions per cap key per window"]
  end

  subgraph pol["Policy applied on the read path"]
    direction LR
    exp["Experiment gate<br/>which ranking variant"]
    hist["Topic affinity<br/>on-device history signal"]
    priv["Privacy gate<br/>what may be read, ever"]
  end

  surf --> query
  query --> gather
  idx --> gather
  rows --> gather
  gather --> filter
  capstore --> filter
  filter --> rank
  exp --> rank
  hist -.-> rank
  priv -.->|"gates"| hist
  rank --> out
  filter -.->|"empty"| miss

  classDef app fill:#F3EDE4,stroke:#8A7A5C,color:#2B2418
  classDef bound fill:#E5ECF2,stroke:#5A7C93,color:#12242E
  classDef dm fill:#FFFFFF,stroke:#4A4A52,color:#1A1A20
  classDef rd fill:#DDEBE9,stroke:#2C6B67,color:#0D2B29
  classDef warn fill:#F5EAE1,stroke:#A0562F,color:#3A1F10
  class surf app
  class query,out bound
  class gather,filter,rank,exp,hist,priv dm
  class idx,rows,capstore rd
  class miss warn
```

> **The read path is the contract.** `query()` never touches the network, never blocks,
> and never throws. It reads the store and returns whatever is eligible — possibly nothing.
> A miss is not an error; it is a signal to the worker that this placement needs filling.
> That is what makes the API Sentry-shaped: errors become telemetry, not consumer control flow.

### 3.4 Interaction path — impressions, clicks, reports

```mermaid
%%{init: {"theme":"base","themeVariables":{"fontSize":"14px","lineColor":"#5A6C69","textColor":"#16211F","clusterBkg":"#EFF3F1","clusterBorder":"#C3D0CC","edgeLabelBackground":"#F8FAF9"}}}%%
flowchart TB
  subgraph app["Consumer app"]
    direction LR
    view["View layer<br/>ad view attached, on screen"]
    tap["User tap"]
    menu["Report control"]
  end

  subgraph det["Detection — moved into the SDK"]
    direction LR
    vis["Viewability signal<br/>raw: visible fraction, duration"]
    thr["Impression rule<br/>one threshold for every surface"]
  end

  cmdapi["Command API<br/>record interaction"]
  outbox["Interaction outbox<br/>pending impressions · clicks · reports"]
  cbd["Callback dispatcher<br/>drains · retries with backoff · dedupes"]
  cbs["Callback endpoints<br/>impression · click · report"]
  capstore["Cap counters<br/>updated only on confirmed impression"]

  view -->|"visible for N ms"| vis
  vis --> thr
  thr -->|"impression"| cmdapi
  tap -->|"click"| cmdapi
  menu -->|"report + reason"| cmdapi
  cmdapi -->|"append"| outbox
  outbox --> cbd
  cbd -->|"viaduct GET"| cbs
  cbd -->|"confirmed"| capstore

  classDef appc fill:#F3EDE4,stroke:#8A7A5C,color:#2B2418
  classDef dm fill:#FFFFFF,stroke:#4A4A52,color:#1A1A20
  classDef rd fill:#DDEBE9,stroke:#2C6B67,color:#0D2B29
  classDef eg fill:#F5EAE1,stroke:#A0562F,color:#3A1F10
  classDef ext fill:#EFE6EC,stroke:#8A6A80,color:#2B1A26
  class view,tap,menu appc
  class vis,thr dm
  class cmdapi,outbox,capstore rd
  class cbd eg
  class cbs ext
```

The consumer stops declaring impressions and starts reporting raw visibility. The
threshold moves inside, which is what makes cap counters comparable across surfaces.

### 3.5 Block responsibilities

| Block | Owns | Explicitly does **not** own |
| --- | --- | --- |
| Command API | Validating and deduping intent; turning a call into a durable command | Any network work inline |
| Command queue | Durability and ordering; surviving process death | Deciding *when* work runs |
| Background worker | The only thread that talks to the network; draining the queue; scheduled ticks | Business rules — it asks the domain services |
| Placement registry | What placements exist, their shape and count | Fetching |
| Renewal policy | What is stale, what expires soon, when to refresh ahead of expiry | Fetching |
| Frequency capping | Counting impressions per cap key and window; declaring an ad ineligible | Deciding what a view *is* |
| Selection and ranking | "Best stored ad for this placement, now" | Fetching, or any network on the read path |
| Experiment gate | Resolving Nimbus variants into policy parameters | Enrolment — Nimbus owns that |
| Privacy gate | The hard boundary on which local signals may be read, and what may leave the device | Being optional |
| Ads store | Ad rows and their state; placement index; cap counters | Being a general HTTP cache |
| Interaction outbox | Durable, retried, deduped delivery of impressions, clicks and reports | Deciding when an impression occurred |
| Callback dispatcher | Draining the outbox with backoff | Being called synchronously by the UI |
| MARS client | Request shape, context id, response parsing | Deciding when to fetch |

---

## 4. The life of one ad

Renewal and capping are only expressible because a stored ad has states. This is the
mechanism the HTTP cache cannot represent: a blob is either present or expired, full stop.

```mermaid
%%{init: {"theme":"base","themeVariables":{"fontSize":"14px","lineColor":"#5A6C69","textColor":"#16211F","primaryColor":"#DDEBE9","primaryBorderColor":"#2C6B67","primaryTextColor":"#0D2B29","labelBackgroundColor":"#F8FAF9"}}}%%
stateDiagram-v2
  [*] --> Requested: worker dispatches fetch
  Requested --> Fresh: projected into store
  Requested --> [*]: no fill
  Fresh --> Served: selected by query()
  Served --> Impressed: viewability threshold met
  Impressed --> Fresh: still eligible, under cap
  Impressed --> Capped: cap key exhausted for the window
  Fresh --> Expiring: within the refresh horizon
  Expiring --> Renewed: worker refetches the placement
  Renewed --> Fresh
  Expiring --> Expired: TTL passed, no renewal
  Capped --> Evicted: window rolls over, or GC
  Expired --> Evicted
  Evicted --> [*]
```

`Expiring → Renewed` is the auto-renew loop; `Impressed → Capped` is frequency capping.
Both are the worker acting on store state, with no consumer involvement.

---

## 5. A session, end to end

```mermaid
%%{init: {"theme":"base","themeVariables":{"fontSize":"14px","lineColor":"#5A6C69","textColor":"#16211F","actorBkg":"#E5ECF2","actorBorder":"#5A7C93","actorTextColor":"#12242E","noteBkgColor":"#DDEBE9","noteBorderColor":"#2C6B67","noteTextColor":"#0D2B29","signalColor":"#3B4B48","signalTextColor":"#16211F","labelBoxBkgColor":"#EFF3F1","labelBoxBorderColor":"#C3D0CC","labelTextColor":"#16211F","sequenceNumberColor":"#FFFFFF"}}}%%
sequenceDiagram
  autonumber
  participant App as Consumer app
  participant API as Ads client API
  participant Q as Command queue
  participant W as Background worker
  participant D as Domain services
  participant S as Ads store
  participant M as MARS

  App->>API: start(config)
  API->>W: spawn worker
  W->>Q: begin draining
  App->>API: dispatch(prefetch: home surface)
  API->>Q: enqueue
  API-->>App: returns immediately
  Q->>W: prefetch command
  W->>D: which placements, how many, which variant?
  D-->>W: fetch plan
  W->>M: fetch
  M-->>W: ads
  W->>S: project into ad rows
  S-->>API: observer — placement filled
  API-->>App: onAdsChanged(placement)

  Note over App,S: later, on render — no network involved
  App->>API: query(placement)
  API->>D: eligible? capped? which variant?
  D->>S: read index + cap counters
  S-->>API: ad
  API-->>App: ad, or nothing — never an error

  Note over App,W: the impression is detected, not declared
  App->>API: view attached, visible
  API->>Q: enqueue interaction
  Q->>W: drain
  W->>S: increment cap counter, write outbox
  W->>M: impression callback, retried and deduped

  Note over W: and on its own schedule
  W->>S: which ads expire soon?
  S-->>W: list
  W->>M: renew
```

Compared with §1.2, the consumer makes three kinds of call — **start**, **dispatch**,
**query** — none of which blocks on the network, and none of which can fail in a way the
UI has to handle.

---

## 6. What's next — phased

Each phase is independently shippable and independently reversible. The arrows are hard
dependencies, not a wish-ordering.

```mermaid
%%{init: {"theme":"base","themeVariables":{"fontSize":"14px","lineColor":"#5A6C69","textColor":"#16211F","edgeLabelBackground":"#F8FAF9"}}}%%
flowchart TB
  p0["Phase 0 — today<br/>stateless HTTP client<br/>+ response cache"]
  p1["Phase 1<br/>dispatch / query<br/>store · queue · worker<br/>behaviour parity"]
  p2["Phase 2<br/>auto-renew + prefetch<br/>expiry horizon · warm placements"]
  p3["Phase 3<br/>frequency capping and pacing<br/>cap counters · rotation"]
  p4["Phase 4<br/>impression detection in the SDK<br/>viewability from the view layer"]
  p5["Phase 5<br/>history-informed selection<br/>on-device topic affinity"]
  p6["Phase 6<br/>experiments<br/>Nimbus-driven policy parameters"]

  p0 -->|"replace the HTTP cache with an ads store"| p1
  p1 -->|"worker gains a schedule"| p2
  p1 -->|"the store can count per ad"| p3
  p3 -->|"caps need a trustworthy view event"| p4
  p1 -->|"selection becomes a local decision"| p5
  p2 -.-> p6
  p3 -.->|"caps and pacing become variants"| p6
  p5 -.-> p6

  classDef done fill:#F5EAE1,stroke:#A0562F,color:#3A1F10
  classDef core fill:#DDEBE9,stroke:#2C6B67,color:#0D2B29
  classDef later fill:#FFFFFF,stroke:#6B5B93,color:#1E1830
  class p0 done
  class p1,p2,p3 core
  class p4,p5,p6 later
```

### Phase 1 — dispatch / query, with parity

Land the skeleton with no new behaviour: command API, durable queue, worker thread, ads
store, projection, observer callback. The HTTP cache is deleted; the store replaces it.
The success criterion is boring on purpose — **surfaces render the same ads, and the
consumer deletes its threading and error-handling code.**

*Depends on: nothing. Unlocks: everything.*

### Phase 2 — auto-renew and prefetch

The worker gains a schedule. It refreshes placements before they expire rather than on
demand, and prefetches on lifecycle signals — app foregrounded, surface about to be shown.
First phase where `query()` is expected to be a store hit essentially always.

*Depends on: P1 worker and store.*

### Phase 3 — frequency capping and pacing

Cap counters become first-class store state. The `caps` already arriving on spocs stop
being decoration and start being enforced on device. Rotation across the stored pool comes
for free once selection is local.

*Depends on: P1 per-ad rows. Credibility gated by: P4.*

### Phase 4 — impression detection inside the SDK

Today the consumer decides what an impression is, and every surface decides it slightly
differently. Move the definition inward: the view layer reports raw visibility, the SDK
applies the threshold, writes the outbox, and increments caps. **This is the phase that
makes capping trustworthy** — caps built on inconsistent impression definitions are not caps.

*Depends on: platform visibility signals on iOS and Android.*

### Phase 5 — history-informed selection

With a pool of stored ads and an on-device topic signal, "which of these do we show" becomes
a local decision. Everything stays on device behind the privacy gate; no new signal leaves
the client. **This phase needs privacy review before design, not after.**

*Depends on: P1 local selection, plus privacy sign-off.*

### Phase 6 — experiments

Once policy lives in named services with parameters — horizon, cap window, threshold,
ranking weights — Nimbus can vary them. Experiments become configuration, not forks.

*Depends on: P2, P3 and P5 having tunable parameters.*

### Beyond the numbered phases

- Multi-surface coordination — one pool, several surfaces, no duplicate ads across them.
- Offline-first — dispatch while offline, drain when connectivity returns.
- Server-assisted pacing — MARS hints, client enforces.

---

## 7. Boundaries

```mermaid
%%{init: {"theme":"base","themeVariables":{"fontSize":"14px","lineColor":"#5A6C69","textColor":"#16211F","clusterBkg":"#EFF3F1","clusterBorder":"#C3D0CC","edgeLabelBackground":"#F8FAF9"}}}%%
flowchart TB
  b1["Consumer app — Kotlin / Swift / JS<br/>Render ads. Report raw view events. Dispatch lifecycle. Nothing else."]
  b2["FFI boundary — UniFFI<br/>start · dispatch · query · observer · telemetry<br/>Narrow and versioned. No cache knobs. No error-driven control flow."]
  b3["Rust component — ads-client<br/>All ads business logic: queue · worker · store · policy · selection · delivery"]
  b4["Network boundary<br/>MARS and callback endpoints. Reached only from the worker thread."]
  b5["Privacy boundary<br/>Local signals may be read for on-device selection.<br/>They never cross the network boundary."]

  b1 --> b2 --> b3 --> b4
  b5 -.->|"constrains"| b3
  b5 -.->|"blocks"| b4

  classDef app fill:#F3EDE4,stroke:#8A7A5C,color:#2B2418
  classDef bound fill:#E5ECF2,stroke:#5A7C93,color:#12242E
  classDef core fill:#DDEBE9,stroke:#2C6B67,color:#0D2B29
  classDef ext fill:#EFE6EC,stroke:#8A6A80,color:#2B1A26
  classDef priv fill:#F5EAE1,stroke:#A0562F,color:#3A1F10
  class b1 app
  class b2 bound
  class b3 core
  class b4 ext
  class b5 priv
```

The privacy boundary is not a layer in the stack — it is a constraint that cuts across the
component and hard-stops at the network edge.

### The responsibility ledger

| Concern | Today | V2 |
| --- | --- | --- |
| When ads are fetched | Consumer calls | SDK worker decides |
| Threading | Consumer | One SDK worker thread |
| Errors | Thrown across the FFI | Telemetry only; `query()` returns nothing |
| Retry and offline | Nobody | Queue + outbox with backoff |
| Cache config | Consumer: path, TTL, size | Gone from the public API |
| What is an impression | Each surface, differently | SDK, one definition |
| Frequency capping | Not enforced | SDK |
| Ad rotation | MARS, per request | SDK, over the stored pool |
| Which ad for which slot | MARS | MARS fills the pool, SDK selects from it |
| Experiments | Per-consumer plumbing | Nimbus → SDK policy parameters |

---

## 8. Open questions

These are the decisions that change the diagrams above. They need owners, not consensus.

1. **Threading model across the FFI.** One worker thread inside Rust, or an async runtime?
   What does the consumer do on Desktop, where the JS layer has its own event loop?
2. **Observer versus polling.** Does the UI get a change callback, or does it simply query
   on render? A callback is nicer but adds a foreign-callback lifecycle to manage.
3. **Store durability.** Does the ads store survive process restart, or is it in-memory with
   only the queue and cap counters persisted? Caps must persist; ad rows arguably should not
   outlive their TTL anyway.
4. **`context_id` and the store.** Rotation currently must not invalidate the cache. With
   per-ad rows, what happens to stored ads when the context id rotates?
5. **Impression definition.** What visibility threshold, on which platforms, and can iOS and
   Android both supply the raw signal the SDK needs?
6. **Privacy review scope.** Phase 5 needs sign-off before design work starts. Is a coarse
   on-device topic signal acceptable, and who owns that determination?
7. **Migration.** Do we ship v2 behind a Nimbus flag with the v1 API still present, or cut
   over per surface?
8. **Fill guarantees.** If `query()` can return nothing, what does each surface render in the
   empty state, and is that acceptable to the business?
