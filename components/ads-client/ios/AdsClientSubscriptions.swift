/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// NOTE: This is the iOS counterpart to `AdsClientSubscriptions.kt`. To ship it,
// it needs to compile alongside the generated bindings in the
// `MozillaRustComponentsWrapper` module (where `MozAdsClient` et al. live); it
// lives here next to the component as the reference implementation.

import Foundation

public extension MozAdsClient {
    /// Observe a placement's tile ad as an `AsyncStream`.
    ///
    /// The first value is the cached ad if one exists (instant), then the fresh
    /// ad once MARS responds; `nil` means no fill. Terminating the stream
    /// unsubscribes in Rust. Values arrive on the worker thread — hop to
    /// `@MainActor` to render.
    func tileAdStream(placementId: String) -> AsyncStream<MozAdsTile?> {
        AsyncStream { continuation in
            let subscription = subscribeTileAd(placementId: placementId, subscriber: TileSink(continuation))
            continuation.onTermination = { _ in subscription.unsubscribe() }
        }
    }

    /// Observe a placement's image ad as an `AsyncStream`. See `tileAdStream`.
    func imageAdStream(placementId: String) -> AsyncStream<MozAdsImage?> {
        AsyncStream { continuation in
            let subscription = subscribeImageAd(placementId: placementId, subscriber: ImageSink(continuation))
            continuation.onTermination = { _ in subscription.unsubscribe() }
        }
    }

    /// Observe a placement's spoc ads as an `AsyncStream`. Empty means no fill. See `tileAdStream`.
    func spocAdsStream(placementId: String) -> AsyncStream<[MozAdsSpoc]> {
        AsyncStream { continuation in
            let subscription = subscribeSpocAds(placementId: placementId, subscriber: SpocSink(continuation))
            continuation.onTermination = { _ in subscription.unsubscribe() }
        }
    }
}

// Bridge the UniFFI callbacks into the stream continuations.

private final class TileSink: MozAdsTileSubscriber {
    private let continuation: AsyncStream<MozAdsTile?>.Continuation
    init(_ continuation: AsyncStream<MozAdsTile?>.Continuation) { self.continuation = continuation }
    func onAds(ad: MozAdsTile?) { continuation.yield(ad) }
}

private final class ImageSink: MozAdsImageSubscriber {
    private let continuation: AsyncStream<MozAdsImage?>.Continuation
    init(_ continuation: AsyncStream<MozAdsImage?>.Continuation) { self.continuation = continuation }
    func onAds(ad: MozAdsImage?) { continuation.yield(ad) }
}

private final class SpocSink: MozAdsSpocSubscriber {
    private let continuation: AsyncStream<[MozAdsSpoc]>.Continuation
    init(_ continuation: AsyncStream<[MozAdsSpoc]>.Continuation) { self.continuation = continuation }
    func onAds(ads: [MozAdsSpoc]) { continuation.yield(ads) }
}
