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

/**
 * Records the ads client's success metrics, expected errors and unexpected
 * errors in Glean. Pass it to `MozAdsClientBuilder.telemetrySink()`.
 */
class AdsClientTelemetrySink : MozAdsTelemetrySink {
    override fun accumulateRequestDuration(duration: MozAdsRequestDuration, millis: Long) {
        val metric = when (duration) {
            MozAdsRequestDuration.DIRECT -> AdsClient.requestDurationDirect
            MozAdsRequestDuration.OHTTP -> AdsClient.requestDurationOhttp
        }
        metric.accumulateSingleSample(millis)
    }

    override fun accumulateContextIdLifetime(minutes: Long) {
        AdsClient.contextIdLifetime.accumulateSingleSample(minutes)
    }

    override fun addToRate(rate: MozAdsRate, numerator: Int, denominator: Int) {
        val metric = when (rate) {
            MozAdsRate.CACHE_HIT -> AdsClient.cacheHitRate
            MozAdsRate.CLICK_SUCCESS -> AdsClient.clickSuccessRate
            MozAdsRate.FILL -> AdsClient.fillRate
            MozAdsRate.IMPRESSION_SUCCESS -> AdsClient.impressionSuccessRate
            MozAdsRate.REQUEST_SUCCESS -> AdsClient.requestSuccessRate
        }
        metric.addToNumerator(numerator)
        metric.addToDenominator(denominator)
    }

    override fun recordExpectedError(error: MozAdsExpectedError) {
        AdsClient.expectedError.record(
            AdsClient.ExpectedErrorExtra(
                code = error.code,
                fatal = error.fatal,
                kind = error.kind,
                ohttp = error.ohttp,
                operation = error.operation,
                stage = error.stage,
            ),
        )
    }

    override fun submitErrorReports(json: String, reason: String) {
        // The JSON already matches the metric's structure, so it is set as is
        // rather than rebuilt into the generated classes.
        AdsClient.errorReports.inner.setString(json)
        Pings.adsClientErrors.submit(
            Pings.adsClientErrorsReasonCodes.values().firstOrNull { it.name == reason },
        )
    }
}
