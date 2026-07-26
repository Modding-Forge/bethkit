// SPDX-License-Identifier: Apache-2.0
//!
//! Converts one normalized xEdit export into a candidate schema package.

use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use bethkit_schema::{convert_xedit_export, ConversionRules, XEditExport};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments.len() != 3 {
        return Err(
            "usage: bethkit-xedit-converter <export.json> <rules.json> <output.bkschema>".into(),
        );
    }
    let export_path = PathBuf::from(&arguments[0]);
    let rules_path = PathBuf::from(&arguments[1]);
    let output_path = PathBuf::from(&arguments[2]);
    let export: XEditExport = read_json(&export_path)?;
    let rules_bytes = fs::read(&rules_path)?;
    let rules: ConversionRules = serde_json::from_slice(&rules_bytes)?;
    let package = convert_xedit_export(export, &rules, &rules_bytes)?;
    fs::write(output_path, package.to_bytes()?)?;
    Ok(())
}

fn read_json<T>(path: &Path) -> Result<T, Box<dyn Error>>
where
    T: serde::de::DeserializeOwned,
{
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
