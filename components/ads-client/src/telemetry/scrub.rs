/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

//! Removes values from an error message before it is recorded, keeping its
//! shape: `invalid type: string "https://…", expected u32` becomes
//! `invalid type: string <value>, expected u32`.
//!
//! Only serde messages reach this: the classifier never records any other
//! error's `Display`. It is a second line of defence, not the first.

/// Longest message recorded, in bytes.
pub const MAX_MESSAGE_BYTES: usize = 512;

pub fn scrub(message: &str) -> String {
    let message = message.strip_prefix("[no-sentry] ").unwrap_or(message);
    let mut scrubbed = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(c) = rest.chars().next() {
        let consumed = match c {
            '"' => {
                scrubbed.push_str("<value>");
                quoted_len(rest)
            }
            '`' => {
                let len = rest[1..].find('`').map_or(rest.len(), |end| end + 2);
                let token = rest[..len].trim_matches('`');
                if is_identifier(token) {
                    scrubbed.push_str(&rest[..len]);
                } else {
                    scrubbed.push_str("<value>");
                }
                len
            }
            _ => {
                let len = rest
                    .find(|c: char| c.is_whitespace() || matches!(c, '"' | '`' | ',' | '(' | ')'))
                    .unwrap_or(rest.len())
                    .max(c.len_utf8());
                scrubbed.push_str(scrub_word(&rest[..len]));
                len
            }
        };
        rest = &rest[consumed..];
    }
    truncate(scrubbed, MAX_MESSAGE_BYTES)
}

pub fn truncate(mut value: String, max_bytes: usize) -> String {
    if value.len() > max_bytes {
        let mut end = max_bytes;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
    }
    value
}

/// Length of the Debug-formatted string literal `quoted` starts with, quotes
/// and escapes included.
fn quoted_len(quoted: &str) -> usize {
    let mut escaped = false;
    for (i, c) in quoted.char_indices().skip(1) {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return i + 1,
            _ => {}
        }
    }
    quoted.len()
}

fn is_identifier(token: &str) -> bool {
    let mut chars = token.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))
        && token.len() <= 64
        && !matches!(token, "true" | "false")
}

fn scrub_word(word: &str) -> &str {
    if word.contains("://") {
        "<url>"
    } else if is_path(word) {
        "<path>"
    } else if word.chars().filter(char::is_ascii_hexdigit).count() >= 12
        && word.chars().any(|c| c.is_ascii_digit())
    {
        "<n>"
    } else {
        word
    }
}

fn is_path(word: &str) -> bool {
    let bytes = word.as_bytes();
    (word.starts_with('/') && word[1..].contains('/'))
        || word.starts_with("\\\\")
        || (bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && &bytes[1..3] == b":\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hostile_inputs() {
        let cases = [
            (
                r#"invalid type: string "https://ads.test/c?id=1", expected u32"#,
                "invalid type: string <value>, expected u32",
            ),
            (
                r#"invalid value: string "a \"quoted\" https://x.test", expected a URL"#,
                "invalid value: string <value>, expected a URL",
            ),
            (
                "invalid type: integer `12`, expected a string",
                "invalid type: integer <value>, expected a string",
            ),
            (
                "invalid type: boolean `true`, expected a string",
                "invalid type: boolean <value>, expected a string",
            ),
            (
                "missing field `callbacks` at line 1 column 42",
                "missing field `callbacks` at line 1 column 42",
            ),
            (
                "unknown variant `spocx`, expected one of `spoc`, `tile`",
                "unknown variant `spocx`, expected one of `spoc`, `tile`",
            ),
            (
                "failed at https://ads.test/v1/t?cap_key=2 then",
                "failed at <url> then",
            ),
            (
                "cannot open /Users/someone/Library/ads.sqlite",
                "cannot open <path>",
            ),
            (
                r"cannot open C:\Users\someone\ads.sqlite",
                "cannot open <path>",
            ),
            (
                "id 0f6a9c1e-55b3-4c4e-9d2e-2b1c4a7e9f00 rejected",
                "id <n> rejected",
            ),
            ("[no-sentry] Backend error: x", "Backend error: x"),
        ];
        for (input, expected) in cases {
            assert_eq!(scrub(input), expected, "scrubbing {input:?}");
        }
    }

    #[test]
    fn test_truncates_on_a_char_boundary() {
        let scrubbed = scrub(&"é".repeat(400));
        assert!(scrubbed.len() <= MAX_MESSAGE_BYTES);
        assert!(scrubbed.chars().all(|c| c == 'é'));
    }
}
