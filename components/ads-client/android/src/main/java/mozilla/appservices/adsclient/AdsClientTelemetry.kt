/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.adsclient

import mozilla.appservices.adsclient.MozAdsTelemetry
import org.mozilla.appservices.adsclient.GleanMetrics.AdsClient

/**
 * AdsClientTelemetry is a thin wrapper used to expose
 * callbacks used to emit telemetry events to Glean.
 */
@Deprecated(
    "Pass AdsClientTelemetrySink to MozAdsClientBuilder.telemetrySink() instead",
    ReplaceWith("AdsClientTelemetrySink()"),
)
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

class AdsClientTelemetrySink : MozAdsTelemetrySink {
    override fun recordRequestSuccessRate(numerator: Int, denominator: Int) {
        AdsClient.requestSuccessRate.addToNumerator(numerator)
        AdsClient.requestSuccessRate.addToDenominator(denominator)
    }

    override fun recordImpressionSuccessRate(numerator: Int, denominator: Int) {
        AdsClient.impressionSuccessRate.addToNumerator(numerator)
        AdsClient.impressionSuccessRate.addToDenominator(denominator)
    }

    override fun recordClickSuccessRate(numerator: Int, denominator: Int) {
        AdsClient.clickSuccessRate.addToNumerator(numerator)
        AdsClient.clickSuccessRate.addToDenominator(denominator)
    }

    override fun recordCacheHitRate(numerator: Int, denominator: Int) {
        AdsClient.cacheHitRate.addToNumerator(numerator)
        AdsClient.cacheHitRate.addToDenominator(denominator)
    }

    override fun recordRequestDurationDirect(millis: Long) {
        AdsClient.requestDurationDirect.accumulateSingleSample(millis)
    }

    override fun recordRequestDurationOhttp(millis: Long) {
        AdsClient.requestDurationOhttp.accumulateSingleSample(millis)
    }

    override fun recordContextIdLifetime(minutes: Long) {
        AdsClient.contextIdLifetime.accumulateSingleSample(minutes)
    }

    override fun recordExpectedError(operation: String?, kind: String, code: String?) {
        AdsClient.expectedError.record(
            AdsClient.ExpectedErrorExtra(operation = operation, kind = kind, code = code),
        )
    }

    override fun recordUnexpectedError(operation: String?, kind: String, code: String?, detail: String?) {
        AdsClient.unexpectedError.record(
            AdsClient.UnexpectedErrorExtra(operation = operation, kind = kind, code = code, detail = detail),
        )
    }
}
