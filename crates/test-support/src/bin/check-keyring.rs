use anyhow::{Context, Result, bail};
use mindcanary_storage::{DatabaseKeyProvider, OsKeyringKeyProvider};

fn main() -> Result<()> {
    let expected = parse_expected()?;
    let present = OsKeyringKeyProvider
        .load()
        .context("load MindCanary OS-keyring entry")?
        .is_some();

    match (expected.as_str(), present) {
        ("present", true) => {
            println!("MindCanary OS-keyring entry present");
            Ok(())
        }
        ("absent", false) => {
            println!("MindCanary OS-keyring entry absent");
            Ok(())
        }
        ("present", false) => bail!("expected MindCanary OS-keyring entry to be present"),
        ("absent", true) => bail!("expected MindCanary OS-keyring entry to be absent"),
        _ => bail!("unknown expectation {expected:?}"),
    }
}

fn parse_expected() -> Result<String> {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next(), args.next()) {
        (Some("--expect"), Some(value), None) if value == "present" || value == "absent" => {
            Ok(value)
        }
        _ => bail!("Usage: check-keyring --expect <present|absent>"),
    }
}
