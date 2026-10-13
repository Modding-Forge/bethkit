// SPDX-License-Identifier: Apache-2.0
//! Detector byte fidelity and fail-closed override regression coverage.

use super::*;

/// Verifies the temporary sampling codec preserves every byte value exactly.
#[test]
fn sampling_codec_is_reversible() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let bytes: Vec<u8> = (0..=255).collect();
    let (text, decode_errors) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(&bytes);
    let (encoded, _, encode_errors) = encoding_rs::WINDOWS_1252.encode(&text);
    assert!(!decode_errors && !encode_errors);
    assert_eq!(encoded.as_ref(), bytes);
    Ok(())
}

/// Recognizes UTF-8 and Cyrillic legacy text from meaningful byte samples.
#[test]
fn detector_recognizes_cyrillic_samples() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let text =
        "Это длинный текст задания. Найдите древний меч и вернитесь к хозяину таверны. ".repeat(8);
    for codec in [encoding_rs::UTF_8, encoding_rs::WINDOWS_1251] {
        let (bytes, _, errors) = codec.encode(&text);
        assert!(!errors);
        let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
        detector.feed(&bytes, true);
        assert_eq!(detector.guess(None, Utf8Detection::Allow), codec);
    }
    Ok(())
}

/// Parses explicit codepages and rejects missing or invalid overrides without guessing.
#[test]
fn override_is_explicit_and_fail_closed() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("test.cpoverride");
    assert!(read_override(&path).is_err());
    fs::write(&path, "\u{feff}1251\r\nignored second line")?;
    assert_eq!(read_override(&path)?.schema_name(), "windows-1251");
    for label in ["", "unknown", "utf-16", "50220"] {
        fs::write(&path, label)?;
        assert!(read_override(&path).is_err());
    }
    fs::write(&path, vec![b' '; 4097])?;
    assert!(read_override(&path).is_err());
    Ok(())
}
