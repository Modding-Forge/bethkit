// SPDX-License-Identifier: Apache-2.0
//!
//! Encoding metadata for Bethesda string tables.
//!
//! Bethesda games store strings in single-byte legacy code pages whose choice
//! depends on the language of the `.STRINGS` files. This module exposes the
//! known mapping but does not perform any conversion itself — callers receive
//! raw byte payloads from [`crate::strings::StringTable`] and pick the encoding
//! they need (typically via the `encoding_rs` crate or similar).

/// A legacy code page used by a Bethesda localisation file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodePage {
    /// Windows-1252 — Latin-1 superset used for English, French, German,
    /// Italian, and Spanish.
    Windows1252,

    /// Windows-1251 — Cyrillic, used for the Russian translation.
    Windows1251,

    /// Windows-1250 — Central European, used for Polish, Czech, etc.
    Windows1250,

    /// Shift-JIS — Japanese.
    ShiftJis,

    /// GBK — Simplified Chinese.
    Gbk,

    /// Big5 — Traditional Chinese.
    Big5,
}

impl CodePage {
    /// Returns the Windows code page identifier for this encoding.
    pub fn windows_id(self) -> u32 {
        match self {
            Self::Windows1252 => 1252,
            Self::Windows1251 => 1251,
            Self::Windows1250 => 1250,
            Self::ShiftJis => 932,
            Self::Gbk => 936,
            Self::Big5 => 950,
        }
    }

    /// Returns the canonical IANA / WHATWG label for this encoding.
    pub fn whatwg_label(self) -> &'static str {
        match self {
            Self::Windows1252 => "windows-1252",
            Self::Windows1251 => "windows-1251",
            Self::Windows1250 => "windows-1250",
            Self::ShiftJis => "shift_jis",
            Self::Gbk => "gbk",
            Self::Big5 => "big5",
        }
    }
}

/// Resolves the legacy code page for a Bethesda localisation language tag.
///
/// The `language` argument is matched case-insensitively against the suffix
/// found in `.STRINGS` filenames (for example `English` in
/// `Skyrim_English.STRINGS`).
///
/// Returns [`CodePage::Windows1252`] for unknown languages because that is the
/// only safe fallback for Latin-script content; callers that care about exact
/// fidelity should branch on the returned value.
pub fn code_page_for_language(language: &str) -> CodePage {
    match language.to_ascii_lowercase().as_str() {
        "russian" => CodePage::Windows1251,
        "polish" | "czech" | "hungarian" => CodePage::Windows1250,
        "japanese" => CodePage::ShiftJis,
        "chinese" => CodePage::Gbk,
        "traditionalchinese" | "traditional_chinese" => CodePage::Big5,
        _ => CodePage::Windows1252,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that known languages map to their documented code pages.
    #[test]
    fn known_languages_map_to_expected_code_pages(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let cases: &[(&str, CodePage)] = &[
            ("English", CodePage::Windows1252),
            ("RUSSIAN", CodePage::Windows1251),
            ("polish", CodePage::Windows1250),
            ("Japanese", CodePage::ShiftJis),
            ("unknown", CodePage::Windows1252),
        ];

        // when / then
        for (lang, expected) in cases {
            assert_eq!(code_page_for_language(lang), *expected, "language: {lang}");
        }
        Ok(())
    }

    /// Verifies that the Windows code page identifiers match the historic
    /// Microsoft definitions.
    #[test]
    fn windows_ids_are_correct() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // given
        let cases: &[(CodePage, u32)] = &[
            (CodePage::Windows1252, 1252),
            (CodePage::Windows1251, 1251),
            (CodePage::ShiftJis, 932),
        ];

        // when / then
        for (cp, expected) in cases {
            assert_eq!(cp.windows_id(), *expected);
        }
        Ok(())
    }
}
