use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use mindcanary_test_support::{synthetic_browser_requests, synthetic_check_in_requests};

fn main() -> Result<()> {
    let output_dir = env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("fixtures"), PathBuf::from);
    write_jsonl(
        output_dir.as_path().join("synthetic-browser.jsonl"),
        synthetic_browser_requests(),
    )?;
    write_jsonl(
        output_dir.as_path().join("synthetic-check-ins.jsonl"),
        synthetic_check_in_requests(),
    )
}

fn write_jsonl(
    output_path: impl AsRef<Path>,
    requests: Vec<mindcanary_protocol::ProtocolRequest>,
) -> Result<()> {
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).context("create fixture directory")?;
    }

    let mut output = String::new();
    for request in requests {
        output.push_str(&serde_json::to_string(&request)?);
        output.push('\n');
    }

    fs::write(output_path, output).context("write synthetic fixture")
}
