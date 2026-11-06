/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use chrono::DateTime;
use std::time::Duration;
use viaduct::Response;

/// How old `response` already is at `now_epoch_seconds` (RFC 9111 §4.2.3):
/// the larger of its `Age` header and the time elapsed since its `Date`
/// header. A missing or unreadable header counts as zero.
pub(super) fn current_age(response: &Response, now_epoch_seconds: i64) -> Duration {
    let age = response
        .headers
        .get("age")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let apparent_age = response
        .headers
        .get("date")
        .and_then(|value| DateTime::parse_from_rfc2822(value.trim()).ok())
        .and_then(|date| u64::try_from(now_epoch_seconds.saturating_sub(date.timestamp())).ok())
        .unwrap_or(0);
    Duration::from_secs(age.max(apparent_age))
}

#[cfg(test)]
mod tests {
    use super::*;
    use viaduct::{Headers, Method};

    // Thu, 06 Nov 2025 08:49:37 GMT
    const NOW: i64 = 1_762_418_977;

    fn response_with(headers: &[(&str, &str)]) -> Response {
        let mut map = Headers::new();
        for (name, value) in headers {
            map.insert(name.to_string(), *value).unwrap();
        }
        Response {
            body: vec![],
            headers: map,
            request_method: Method::Get,
            status: 200,
            url: "https://example.com".parse().unwrap(),
        }
    }

    #[test]
    fn no_age_or_date_is_zero() {
        assert_eq!(current_age(&response_with(&[]), NOW), Duration::ZERO);
    }

    #[test]
    fn uses_age_header() {
        let response = response_with(&[("age", "72000")]);
        assert_eq!(current_age(&response, NOW), Duration::from_secs(72000));
    }

    #[test]
    fn uses_time_since_date_header() {
        let response = response_with(&[("date", "Thu, 06 Nov 2025 07:49:37 GMT")]);
        assert_eq!(current_age(&response, NOW), Duration::from_secs(3600));
    }

    #[test]
    fn takes_the_larger_of_age_and_date() {
        let response = response_with(&[("age", "60"), ("date", "Thu, 06 Nov 2025 07:49:37 GMT")]);
        assert_eq!(current_age(&response, NOW), Duration::from_secs(3600));

        let response = response_with(&[("age", "7200"), ("date", "Thu, 06 Nov 2025 07:49:37 GMT")]);
        assert_eq!(current_age(&response, NOW), Duration::from_secs(7200));
    }

    #[test]
    fn date_in_the_future_is_zero() {
        let response = response_with(&[("date", "Thu, 06 Nov 2025 09:49:37 GMT")]);
        assert_eq!(current_age(&response, NOW), Duration::ZERO);
    }

    #[test]
    fn unreadable_headers_are_ignored() {
        let response = response_with(&[("age", "-5"), ("date", "yesterday")]);
        assert_eq!(current_age(&response, NOW), Duration::ZERO);
    }
}
