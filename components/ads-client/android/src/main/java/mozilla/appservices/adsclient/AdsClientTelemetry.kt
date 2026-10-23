/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.adsclient

import mozilla.appservices.adsclient.MozAdsTelemetry
import org.mozilla.appservices.adsclient.GleanMetrics.AdsClient
import org.mozilla.appservices.adsclient.GleanMetrics.Pings

/**
 * AdsClientTelemetry is a thin wrapper used to expose
 * callbacks used to emit telemetry events to Glean.
 */
class AdsClientTelemetry : MozAdsTelemetry {
    override fun recordClientError(label: String, value: String) {
        AdsClient.clientError[label].set(value)
    }

    override fun recordClientOperationTotal(label: String) {
        AdsClient.clientOperationTotal[label].add()
    }

    override fun recordHttpCacheOutcome(label: String, value: String) {
        AdsClient.httpCacheOutcome[label].set(value)
    }

    override fun recordUnexpectedError(kind: String, message: String) {
        AdsClient.unexpectedError.set(AdsClient.UnexpectedErrorObject(kind = kind, message = message))
        Pings.adsClientErrors.submit()
    }
}
