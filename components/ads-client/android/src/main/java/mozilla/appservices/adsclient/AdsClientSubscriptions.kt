/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.adsclient

import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow

/**
 * Observe a placement's tile ad as a [Flow].
 *
 * The first value is the cached ad if one exists (instant), then the fresh ad
 * once MARS responds; `null` means no fill. Cancelling collection unsubscribes
 * in Rust. Values arrive on the worker thread — use `flowOn`/the main dispatcher
 * to render.
 */
fun MozAdsClient.tileAdFlow(placementId: String): Flow<MozAdsTile?> = callbackFlow {
    val subscription = subscribeTileAd(
        placementId,
        object : MozAdsTileSubscriber {
            override fun onAds(ad: MozAdsTile?) {
                trySend(ad)
            }
        },
    )
    awaitClose { subscription.unsubscribe() }
}

/** Observe a placement's image ad as a [Flow]. See [tileAdFlow]. */
fun MozAdsClient.imageAdFlow(placementId: String): Flow<MozAdsImage?> = callbackFlow {
    val subscription = subscribeImageAd(
        placementId,
        object : MozAdsImageSubscriber {
            override fun onAds(ad: MozAdsImage?) {
                trySend(ad)
            }
        },
    )
    awaitClose { subscription.unsubscribe() }
}

/** Observe a placement's spoc ads as a [Flow]. Empty list means no fill. See [tileAdFlow]. */
fun MozAdsClient.spocAdsFlow(placementId: String): Flow<List<MozAdsSpoc>> = callbackFlow {
    val subscription = subscribeSpocAds(
        placementId,
        object : MozAdsSpocSubscriber {
            override fun onAds(ads: List<MozAdsSpoc>) {
                trySend(ads)
            }
        },
    )
    awaitClose { subscription.unsubscribe() }
}
