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
}

impl InlineStringEncoding {
    /// Returns the stable schema-compatible name of this encoding or policy.
    pub const fn schema_name(self) -> &'static str {
        match self {
            Self::Utf8 => "utf8",
            Self::Windows1252 => "windows_1252",
            Self::PreferUtf8 => "prefer_utf8",
        }
    }
}

impl fmt::Display for InlineStringEncoding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.schema_name())
    }
}
