fn main() -> std::process::ExitCode {
    procwho_mcp::main(&std::env::args().skip(1).collect::<Vec<_>>())
}
