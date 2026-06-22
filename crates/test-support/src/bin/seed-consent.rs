use std::{env, path::PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use mindcanary_protocol::SignalId;
use mindcanary_storage::{EncryptedStore, OsKeyringKeyProvider};

#[derive(Debug)]
struct Arguments {
    database: PathBuf,
    enabled_at: DateTime<Utc>,
    browser_starter: bool,
}

fn main() -> Result<()> {
    let arguments = parse_arguments()?;
    if !arguments.browser_starter {
        bail!("choose a consent group to seed, such as --browser-starter");
    }

    let mut store = EncryptedStore::bootstrap(&arguments.database, &OsKeyringKeyProvider)
        .with_context(|| format!("open database {}", arguments.database.display()))?;

    if arguments.browser_starter {
        for signal in SignalId::BROWSER_STARTER_SET {
            store
                .set_signal_collection(signal, true, arguments.enabled_at)
                .with_context(|| format!("enable {}", signal.as_str()))?;
        }
        println!(
            "Seeded {} browser starter signals at {}",
            SignalId::BROWSER_STARTER_SET.len(),
            arguments.enabled_at.to_rfc3339()
        );
    }

    Ok(())
}

fn parse_arguments() -> Result<Arguments> {
    let mut database: Option<PathBuf> = None;
    let mut enabled_at: Option<DateTime<Utc>> = None;
    let mut browser_starter = false;
    let mut args = env::args_os().skip(1);

    while let Some(argument) = args.next() {
        match argument.to_string_lossy().as_ref() {
            "--database" => {
                database = Some(
                    args.next()
                        .map(PathBuf::from)
                        .context("--database requires a path")?,
                );
            }
            "--enabled-at" => {
                let raw = args
                    .next()
                    .context("--enabled-at requires an RFC3339 timestamp")?;
                let raw = raw.to_string_lossy();
                enabled_at = Some(
                    DateTime::parse_from_rfc3339(&raw)
                        .context("--enabled-at must be RFC3339")?
                        .with_timezone(&Utc),
                );
            }
            "--browser-starter" => browser_starter = true,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => bail!("unknown argument {other:?}"),
        }
    }

    Ok(Arguments {
        database: database.context("--database is required")?,
        enabled_at: enabled_at.context("--enabled-at is required")?,
        browser_starter,
    })
}

fn print_usage() {
    println!("Usage: seed-consent --database <path> --enabled-at <rfc3339> --browser-starter");
}
