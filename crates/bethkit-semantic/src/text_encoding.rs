// SPDX-License-Identifier: Apache-2.0
//! Explicit encodings for strings embedded in plugin records.

use std::fmt;

/// Encoding used for schema-localizable inline text instead of the schema default.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InlineStringEncoding {
    /// UTF-8, used by plugins such as the German USSEP ESP translation.
    Utf8,
    /// Windows-1252, the legacy Latin-script schema default.
    Windows1252,
    /// Prefer UTF-8 for each inline string, falling back to its schema encoding.
    PreferUtf8,
    /// Prefer UTF-8 per string, with one detected legacy fallback for other bytes.
    PreferUtf8WithFallback(&'static encoding_rs::Encoding),
    /// Preserve suspicious or weakly evidenced legacy text as undecoded bytes.
    PreserveUtf8WithFallback {
        /// Detected fallback for legacy strings with sufficient evidence.
        encoding: &'static encoding_rs::Encoding,
        /// Whether the sample is too sparse to select a non-default legacy codec.
        uncertain_legacy: bool,
    },
    /// A supported legacy codec selected by detection or an explicit override.
    Legacy(&'static encoding_rs::Encoding),
    /// Use an authoritative codec, retaining invalid text as raw bytes instead of failing.
    PreserveExplicit(&'static encoding_rs::Encoding),
}

impl InlineStringEncoding {
    /// Returns the stable schema-compatible name of this encoding or policy.
    pub fn schema_name(self) -> &'static str {
        match self {
            Self::Utf8 => "utf8",
            Self::Windows1252 => "windows_1252",
            Self::PreferUtf8
            | Self::PreferUtf8WithFallback(_)
            | Self::PreserveUtf8WithFallback { .. } => "prefer_utf8",
            Self::Legacy(encoding) | Self::PreserveExplicit(encoding) => encoding.name(),
        }
    }

    /// Returns whether this policy makes a heuristic choice for each string.
    pub fn prefers_utf8(self) -> bool {
        matches!(
            self,
            Self::PreferUtf8
                | Self::PreferUtf8WithFallback(_)
                | Self::PreserveUtf8WithFallback { .. }
        )
    }

    pub(crate) fn fallback_encoding(self) -> Option<Self> {
        match self {
            Self::PreferUtf8 => None,
            Self::PreferUtf8WithFallback(codec) => Self::from_label(codec.name()),
            Self::PreserveUtf8WithFallback { encoding, .. } => Self::from_label(encoding.name()),
            concrete => Some(concrete),
        }
    }

    /// Resolves a codec label or supported Windows codepage number.
    ///
    /// Returns `None` for unknown labels, UTF-16, or stateful ISO-2022-JP.
    pub fn from_label(label: &str) -> Option<Self> {
        let label = label.trim().to_ascii_lowercase();
        let label = label
            .parse::<u16>()
            .map(|number| number.to_string())
            .unwrap_or(label);
        let normalized = match label.as_str() {
            "utf8" | "65001" => "utf-8",
            "windows_1252" | "1252" => "windows-1252",
            "1250" => "windows-1250",
            "1251" => "windows-1251",
            "1253" => "windows-1253",
            "1254" => "windows-1254",
            "1255" => "windows-1255",
            "1256" => "windows-1256",
            "1257" => "windows-1257",
            "1258" => "windows-1258",
            "874" => "windows-874",
            "932" => "shift_jis",
            "936" => "gbk",
            "949" => "euc-kr",
            "950" => "big5",
            "866" => "ibm866",
            other => other,
        };
        let encoding = encoding_rs::Encoding::for_label(normalized.as_bytes())?;
        if encoding == encoding_rs::UTF_8 {
            Some(Self::Utf8)
        } else if encoding == encoding_rs::WINDOWS_1252 {
            Some(Self::Windows1252)
        } else if encoding == encoding_rs::UTF_16LE
            || encoding == encoding_rs::UTF_16BE
            || encoding == encoding_rs::ISO_2022_JP
            || encoding == encoding_rs::REPLACEMENT
        {
            None
        } else {
            Some(Self::Legacy(encoding))
        }
    }
}

impl fmt::Display for InlineStringEncoding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.schema_name())
    }
}
