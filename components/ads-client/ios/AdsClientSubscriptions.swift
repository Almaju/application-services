/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// NOTE: This is the iOS counterpart to `AdsClientSubscriptions.kt`. To ship it,
// it needs to compile alongside the generated bindings in the
// `MozillaRustComponentsWrapper` module (where `MozAdsClient` et al. live); it
// lives here next to the component as the reference implementation.

import Foundation

public struct MozAdsError: Error {
    public let reason: String
}

public extension MozAdsClient {
    /// Observe a placement's tile ad as an `AsyncThrowingStream`.
    ///
    /// The first value is the cached ad if one exists (instant), then the fresh
    /// ad once MARS responds; `nil` means no fill. A failed fetch is thrown into
    /// the stream. Terminating the stream unsubscribes in Rust. Values arrive on
    /// the worker thread — hop to `@MainActor` to render.
    func tileAdStream(placementId: String) -> AsyncThrowingStream<MozAdsTile?, Error> {
        AsyncThrowingStream { continuation in
            let subscription = subscribeTileAd(placementId: placementId, subscriber: TileSink(continuation))
            continuation.onTermination = { _ in subscription.unsubscribe() }
        }
    }

    /// Observe a placement's image ad as an `AsyncThrowingStream`. See `tileAdStream`.
    func imageAdStream(placementId: String) -> AsyncThrowingStream<MozAdsImage?, Error> {
        AsyncThrowingStream { continuation in
            let subscription = subscribeImageAd(placementId: placementId, subscriber: ImageSink(continuation))
            continuation.onTermination = { _ in subscription.unsubscribe() }
        }
    }

    /// Observe a placement's spoc ads as an `AsyncThrowingStream`. Empty means no fill. See `tileAdStream`.
    func spocAdsStream(placementId: String) -> AsyncThrowingStream<[MozAdsSpoc], Error> {
        AsyncThrowingStream { continuation in
            let subscription = subscribeSpocAds(placementId: placementId, subscriber: SpocSink(continuation))
            continuation.onTermination = { _ in subscription.unsubscribe() }
        }
    }
}

// Bridge the UniFFI callbacks into the stream continuations.

private final class TileSink: MozAdsTileSubscriber {
    private let continuation: AsyncThrowingStream<MozAdsTile?, Error>.Continuation
    init(_ continuation: AsyncThrowingStream<MozAdsTile?, Error>.Continuation) { self.continuation = continuation }
    func onAds(ad: MozAdsTile?) { continuation.yield(ad) }
    func onError(reason: String) { continuation.finish(throwing: MozAdsError(reason: reason)) }
}

private final class ImageSink: MozAdsImageSubscriber {
    private let continuation: AsyncThrowingStream<MozAdsImage?, Error>.Continuation
    init(_ continuation: AsyncThrowingStream<MozAdsImage?, Error>.Continuation) { self.continuation = continuation }
    func onAds(ad: MozAdsImage?) { continuation.yield(ad) }
    func onError(reason: String) { continuation.finish(throwing: MozAdsError(reason: reason)) }
}

private final class SpocSink: MozAdsSpocSubscriber {
    private let continuation: AsyncThrowingStream<[MozAdsSpoc], Error>.Continuation
    init(_ continuation: AsyncThrowingStream<[MozAdsSpoc], Error>.Continuation) { self.continuation = continuation }
    func onAds(ads: [MozAdsSpoc]) { continuation.yield(ads) }
    func onError(reason: String) { continuation.finish(throwing: MozAdsError(reason: reason)) }
}
