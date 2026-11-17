# Security

Only the latest release is supported with security fixes.

Report a vulnerability privately through
[GitHub private vulnerability reporting](https://github.com/buddyh/procwho/security/advisories/new).
Use the repository's **Security → Report a vulnerability** flow. Do not put a
secret or a working exploit with personal data in a public issue.

Security issues include:

- A redaction leak in CLI output, local state, exports, or the menu bar app.
- Any path that modifies or signals an observed process, service, or worktree.
- Command injection through label providers or reveal commands.
- MCP output that exposes secrets or bypasses output limits or redaction.

Include the release version, macOS version, affected command, expected behavior,
and a minimal reproduction with synthetic values. Describe the impact and which
output or state file is affected. Do not attach raw process lists, environments,
transcripts, or credentials. If a real credential was exposed, revoke it through
its issuer; a code fix cannot revoke it.
