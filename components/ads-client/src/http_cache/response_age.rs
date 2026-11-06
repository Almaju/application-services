/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::clock::Clock;
use chrono::{DateTime, Utc};
use std::time::Duration;
use viaduct::{header_names, Response};

/// How old a response already was when it was received, from its `Age` and
/// `Date` headers (RFC 9111 §4.2.3). A missing or unreadable header counts
/// as zero.
pub(super) struct ResponseAge {
    age: Duration,
    date: Option<DateTime<Utc>>,
}

impl From<&Response> for ResponseAge {
    fn from(response: &Response) -> Self {
        Self {
            age: response
                .headers
                .get(header_names::AGE)
                .and_then(|value| value.trim().parse().ok())
                .map(Duration::from_secs)
                .unwrap_or_default(),
            date: response
                .headers
                .get(header_names::DATE)
                .and_then(|value| DateTime::parse_from_rfc2822(value.trim()).ok())
                .map(|date| date.to_utc()),
        }
    }
}

impl ResponseAge {
    /// The larger of `Age` and the time elapsed since `Date`, as of `clock`.
    pub(super) fn at(&self, clock: &dyn Clock) -> Duration {
        let since_date = self
            .date
            .zip(DateTime::from_timestamp(clock.now_epoch_seconds(), 0))
            .and_then(|(date, now)| (now - date).to_std().ok())
            .unwrap_or_default();
        self.age.max(since_date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::TestClock;
    use viaduct::{Headers, Method};

    // Thu, 06 Nov 2025 08:49:37 GMT
    const NOW: i64 = 1_762_418_977;

    fn age_at_now(headers: &[(&str, &str)]) -> Duration {
        let mut map = Headers::new();
        for (name, value) in headers {
            map.insert(name.to_string(), *value).unwrap();
        }
        let response = Response {
            body: vec![],
            headers: map,
            request_method: Method::Get,
            status: 200,
            url: "https://example.com".parse().unwrap(),
        };
        ResponseAge::from(&response).at(&TestClock::new(NOW))
    }

    #[test]
    fn no_age_or_date_is_zero() {
        assert_eq!(age_at_now(&[]), Duration::ZERO);
    }

    #[test]
    fn uses_age_header() {
        assert_eq!(age_at_now(&[("age", "72000")]), Duration::from_secs(72000));
    }

    #[test]
    fn uses_time_since_date_header() {
        assert_eq!(
            age_at_now(&[("date", "Thu, 06 Nov 2025 07:49:37 GMT")]),
            Duration::from_secs(3600)
        );
    }

    #[test]
    fn takes_the_larger_of_age_and_date() {
        let date = ("date", "Thu, 06 Nov 2025 07:49:37 GMT");
        assert_eq!(
            age_at_now(&[("age", "60"), date]),
            Duration::from_secs(3600)
        );
        assert_eq!(
            age_at_now(&[("age", "7200"), date]),
            Duration::from_secs(7200)
        );
    }

    #[test]
    fn date_in_the_future_is_zero() {
        assert_eq!(
            age_at_now(&[("date", "Thu, 06 Nov 2025 09:49:37 GMT")]),
            Duration::ZERO
        );
    }

    #[test]
    fn unreadable_headers_are_ignored() {
        assert_eq!(
            age_at_now(&[("age", "-5"), ("date", "yesterday")]),
            Duration::ZERO
        );
    }
}
