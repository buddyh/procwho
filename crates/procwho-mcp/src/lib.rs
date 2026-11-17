//! Bounded, read-only MCP stdio transport. No network listener or async runtime.

mod output;
mod protection;
mod protocol;
mod tools;

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

/// Current stateless revision, with initialization-based compatibility for older clients.
pub const PROTOCOL_VERSION: &str = "2026-07-28";
pub const LEGACY_VERSION: &str = "2025-11-25";
const MAX_INPUT_BYTES: u64 = 1024 * 1024;

/// Serve newline-delimited JSON-RPC until EOF. Flush every response; never log to stdout.
pub fn serve(mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut server = protocol::Server::default();
    loop {
        let mut line = Vec::new();
        let n =
            std::io::Read::take(&mut input, MAX_INPUT_BYTES + 1).read_until(b'\n', &mut line)?;
        if n == 0 {
            return Ok(());
        }
        let response = if n as u64 > MAX_INPUT_BYTES {
            // Drain this frame without retaining an unbounded allocation, then recover.
            if line.last() != Some(&b'\n') {
                loop {
                    line.clear();
                    let n = std::io::Read::take(&mut input, MAX_INPUT_BYTES)
                        .read_until(b'\n', &mut line)?;
                    if n == 0 || line.last() == Some(&b'\n') {
                        break;
                    }
                }
            }
            Some(protocol::error(
                serde_json::Value::Null,
                -32700,
                "Message exceeds 1 MiB",
            ))
        } else {
            match serde_json::from_slice(&line) {
                Ok(request) => server.handle(request),
                Err(_) => Some(protocol::error(
                    serde_json::Value::Null,
                    -32700,
                    "Parse error",
                )),
            }
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
}

/// CLI entry point shared by `procwho mcp` and the standalone `procwho-mcp` binary.
pub fn main(args: &[String]) -> ExitCode {
    if matches!(args, [arg] if arg == "--help" || arg == "-h") {
        eprintln!("procwho mcp\nRead-only MCP server over stdio. Close stdin to exit; no flags or network.");
        return ExitCode::SUCCESS;
    }
    if !args.is_empty() {
        eprintln!("procwho mcp: unexpected arguments; use `procwho mcp` without flags");
        return ExitCode::from(2);
    }
    match serve(io::stdin().lock(), io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("procwho mcp: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_from_invalid_utf8_and_oversized_frames() {
        let mut input = vec![b'x'; MAX_INPUT_BYTES as usize + 50];
        input.extend_from_slice(b"\n\xff\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n");
        let mut output = Vec::new();
        serve(std::io::Cursor::new(input), &mut output).unwrap();
        let rows: Vec<serde_json::Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["error"]["code"], -32700);
        assert_eq!(rows[1]["error"]["code"], -32700);
        assert_eq!(rows[2]["id"], 7);
    }
}
