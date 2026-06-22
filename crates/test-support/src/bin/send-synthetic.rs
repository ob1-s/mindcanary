use std::{
    env,
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use mindcanary_protocol::{IngestDisposition, MAX_FRAME_BYTES, ProtocolRequest, ProtocolResponse};
use mindcanary_test_support::{synthetic_browser_requests, synthetic_check_in_requests};

#[derive(Debug)]
struct Arguments {
    socket: PathBuf,
    browser: bool,
    check_ins: bool,
    repeat_first_browser: bool,
}

#[derive(Debug, Default)]
struct Counts {
    stored: u64,
    duplicate: u64,
    rejected: u64,
    errors: u64,
}

fn main() -> Result<()> {
    let arguments = parse_arguments()?;
    let mut requests = Vec::new();

    if arguments.browser {
        let browser_requests = synthetic_browser_requests();
        if arguments.repeat_first_browser {
            let first = browser_requests
                .first()
                .context("synthetic browser fixtures should not be empty")?
                .clone();
            requests.push(first);
        }
        requests.extend(browser_requests);
    }
    if arguments.check_ins {
        requests.extend(synthetic_check_in_requests());
    }
    if requests.is_empty() {
        bail!("choose at least one fixture set with --browser or --check-ins");
    }

    let mut counts = Counts::default();
    for request in requests {
        let response = send_request(&arguments.socket, &request)
            .with_context(|| format!("send synthetic request to {}", arguments.socket.display()))?;
        observe_response(&mut counts, &response);
    }

    println!("stored={}", counts.stored);
    println!("duplicate={}", counts.duplicate);
    println!("rejected={}", counts.rejected);
    println!("errors={}", counts.errors);

    if counts.errors > 0 || counts.rejected > 0 {
        bail!("synthetic ingestion had rejected or error responses");
    }
    Ok(())
}

fn parse_arguments() -> Result<Arguments> {
    let mut socket = default_socket_path();
    let mut browser = false;
    let mut check_ins = false;
    let mut repeat_first_browser = false;
    let mut args = env::args_os().skip(1);

    while let Some(argument) = args.next() {
        match argument.to_string_lossy().as_ref() {
            "--socket" => {
                socket = args
                    .next()
                    .map(PathBuf::from)
                    .context("--socket requires a path")?;
            }
            "--browser" => browser = true,
            "--check-ins" => check_ins = true,
            "--repeat-first-browser" => repeat_first_browser = true,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => bail!("unknown argument {other:?}"),
        }
    }

    Ok(Arguments {
        socket,
        browser,
        check_ins,
        repeat_first_browser,
    })
}

fn print_usage() {
    println!(
        "Usage: send-synthetic [--socket <path>] [--browser] [--check-ins] [--repeat-first-browser]"
    );
}

fn send_request(socket_path: &Path, request: &ProtocolRequest) -> Result<ProtocolResponse> {
    let payload = serde_json::to_vec(request).context("encode synthetic request")?;
    let mut stream = UnixStream::connect(socket_path).context("connect to daemon socket")?;

    write_frame(&mut stream, &payload)?;
    let response = read_frame(&mut stream)?;
    serde_json::from_slice(&response).context("decode daemon response")
}

fn write_frame(stream: &mut UnixStream, payload: &[u8]) -> Result<()> {
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        bail!("invalid request frame length {}", payload.len());
    }

    let length = u32::try_from(payload.len()).context("request frame too large")?;
    stream
        .write_all(&length.to_be_bytes())
        .context("write frame length")?;
    stream.write_all(payload).context("write frame payload")?;
    stream.flush().context("flush frame")
}

fn read_frame(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let mut length_bytes = [0_u8; 4];
    stream
        .read_exact(&mut length_bytes)
        .context("read frame length")?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        bail!("invalid response frame length {length}");
    }

    let mut payload = vec![0_u8; length];
    stream
        .read_exact(&mut payload)
        .context("read frame payload")?;
    Ok(payload)
}

fn observe_response(counts: &mut Counts, response: &ProtocolResponse) {
    match response {
        ProtocolResponse::IngestAcknowledged { disposition, .. }
        | ProtocolResponse::CheckInAcknowledged { disposition, .. } => {
            observe_disposition(counts, *disposition);
        }
        _ => counts.errors += 1,
    }
}

fn observe_disposition(counts: &mut Counts, disposition: IngestDisposition) {
    match disposition {
        IngestDisposition::Stored | IngestDisposition::StoredFiltered => counts.stored += 1,
        IngestDisposition::Duplicate => counts.duplicate += 1,
        IngestDisposition::DiscardedDisabled => counts.rejected += 1,
    }
}

fn default_socket_path() -> PathBuf {
    let runtime_root = env::var_os("XDG_RUNTIME_DIR").map_or_else(env::temp_dir, PathBuf::from);
    runtime_root.join("mindcanary").join("mindcanaryd.sock")
}
