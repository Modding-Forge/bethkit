// SPDX-License-Identifier: Apache-2.0
//! Conservative opt-in detection of uncertain byte text, never text reconstruction.

use crate::InlineStringEncoding;

/// Whether an invalid UTF-8 body should remain raw under the preservation policy.
pub(crate) fn preserve(bytes: &[u8], policy: Option<InlineStringEncoding>) -> bool {
    let Some(InlineStringEncoding::PreserveUtf8WithFallback {
        uncertain_legacy, ..
    }) = policy
    else {
        return false;
    };
    if std::str::from_utf8(bytes).is_ok() {
        return false;
    }
    if uncertain_legacy
        || bytes
            .iter()
            .any(|byte| matches!(byte, 0x81 | 0x8d | 0x8f | 0x90 | 0x9d))
    {
        return true;
    }
    let non_ascii = bytes.iter().filter(|byte| !byte.is_ascii()).count();
    let mut remaining = bytes;
    let mut valid_non_ascii = 0;
    while let Err(error) = std::str::from_utf8(remaining) {
        let prefix = &remaining[..error.valid_up_to()];
        valid_non_ascii += prefix.iter().filter(|byte| !byte.is_ascii()).count();
        let skip =
            error.valid_up_to() + error.error_len().unwrap_or(remaining.len() - prefix.len());
        remaining = &remaining[skip..];
    }
    valid_non_ascii += remaining.iter().filter(|byte| !byte.is_ascii()).count();
    // This is a conservative warning threshold, not a claim of the original encoding.
    valid_non_ascii >= 6 && valid_non_ascii * 2 >= non_ascii
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps damaged UTF-8 fragments and uncertain names raw while accepting valid UTF-8.
    #[test]
    fn preserves_uncertain_bytes_without_guessing() -> Result<(), Box<dyn std::error::Error>> {
        let policy = Some(InlineStringEncoding::PreserveUtf8WithFallback {
            encoding: encoding_rs::WINDOWS_1252,
            uncertain_legacy: false,
        });
        assert!(preserve(b"\xec\xb2\xb4\xeb\xa0\xa5\xea\xb3?", policy));
        assert!(preserve(b"broken\x81text", policy));
        assert!(!preserve(b"Gr\xfc\xdfe", policy));
        assert!(!preserve("Grüße".as_bytes(), policy));
        assert!(!preserve(
            b"\xec\xb2\xb4\xeb\xa0\xa5\xea\xb3?",
            Some(InlineStringEncoding::Windows1252)
        ));
        let sparse = Some(InlineStringEncoding::PreserveUtf8WithFallback {
            encoding: encoding_rs::WINDOWS_1257,
            uncertain_legacy: true,
        });
        assert!(preserve(b"Gh\xfbra", sparse));
        assert!(!preserve(b"ASCII", sparse));
        Ok(())
    }
}
