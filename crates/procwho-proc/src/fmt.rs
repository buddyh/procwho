//! Small display helpers: units, ages, character-safe truncation.

use crate::model::MIB;

/// Memory as Activity Monitor shows it: `812M` or `1.52G`.
///
/// ```
/// use procwho_proc::fmt::fmt_mem;
/// assert_eq!(fmt_mem(512.0 * 1048576.0), "512M");
/// assert_eq!(fmt_mem(1536.0 * 1048576.0), "1.50G");
/// ```
pub fn fmt_mem(b: f64) -> String {
    let m = MIB as f64;
    if b >= 1024.0 * m {
        format!("{:.2}G", b / m / 1024.0)
    } else {
        format!("{:.0}M", b / m)
    }
}

/// Age of a start time as `3m05s`, `2h10m` or `4d02h`; `?` when unknown.
///
/// ```
/// use procwho_proc::fmt::fmt_age;
/// assert_eq!(fmt_age(1000.0, 1185.0), "3m05s");
/// assert_eq!(fmt_age(0.0, 5.0), "?");
/// ```
pub fn fmt_age(start: f64, now: f64) -> String {
    if start == 0.0 {
        return "?".into();
    }
    let s = (now - start) as i64;
    if s >= 86400 {
        format!("{}d{:02}h", s / 86400, s % 86400 / 3600)
    } else if s >= 3600 {
        format!("{}h{:02}m", s / 3600, s % 3600 / 60)
    } else {
        format!("{}m{:02}s", s / 60, s % 60)
    }
}

/// First `n` characters.
pub fn trunc(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Last `n` characters.
pub fn tail_chars(s: &str, n: usize) -> String {
    let c = s.chars().count();
    s.chars().skip(c.saturating_sub(n)).collect()
}

/// Display-safe form of a process-controlled string: every control character (C0, DEL, C1,
/// including ESC and newline) becomes `?`; a tab becomes a space.
///
/// ```
/// use procwho_proc::fmt::sanitize;
/// assert_eq!(sanitize("a\x1b[2Jb\tc\nd"), "a?[2Jb c?d");
/// ```
pub fn sanitize(s: &str) -> String {
    clean(s, false)
}

/// [`sanitize`] for a whole rendered text block: newlines are kept, everything else is filtered.
pub fn clean_text(s: &str) -> String {
    let redacted = s
        .split('\n')
        .map(|line| crate::redact::redact_with_limit(line, usize::MAX))
        .collect::<Vec<_>>()
        .join("\n");
    clean(&redacted, true)
}

fn clean(s: &str, keep_newline: bool) -> String {
    if !s.chars().any(char::is_control) {
        return s.to_string();
    }
    s.chars()
        .map(|c| match c {
            '\t' => ' ',
            '\n' if keep_newline => '\n',
            c if c.is_control() => '?',
            c => c,
        })
        .collect()
}

/// Quotes one word for a POSIX shell. Plain words stay as they are; anything else is
/// wrapped in single quotes with each `'` written as `'\''`.
///
/// ```
/// use procwho_proc::fmt::shell_quote;
/// assert_eq!(shell_quote("/a/b.txt"), "/a/b.txt");
/// assert_eq!(shell_quote("it's here"), r"'it'\''s here'");
/// ```
pub fn shell_quote(s: &str) -> String {
    let plain = !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&c));
    if plain {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_are_filtered() {
        assert_eq!(sanitize("x\x1b[2J\u{9b}31m\x07\x7f"), "x?[2J?31m??");
        assert_eq!(clean_text("a\x1b[2J\nb\r\n"), "a?[2J\nb?\n");
        assert_eq!(sanitize("plain ünïcode"), "plain ünïcode");
    }

    #[test]
    fn shell_quote_survives_hostile_names() {
        let dir = std::env::temp_dir().join(format!("procwho-quote-{}", std::process::id()));
        let marker = dir.join("x");
        let hostile = format!("/tmp/a'; touch {}; '", marker.display());
        let _ = std::fs::create_dir_all(&dir);
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("printf %s {}", shell_quote(&hostile)))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), hostile);
        assert!(!marker.exists(), "quoted value was executed");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn char_safe_truncation() {
        assert_eq!(trunc("héllo wörld", 5), "héllo");
        assert_eq!(tail_chars("héllo wörld", 5), "wörld");
        assert_eq!(trunc("ab", 5), "ab");
    }
}
