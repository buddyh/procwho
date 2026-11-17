//! App bundle paths used by the read-only workload views.

use crate::model::Proc;
use crate::snapshot::Snapshot;
use std::path::{Component, Path, PathBuf};

/// Outermost `.app` component of an absolute executable path.
pub fn outer_bundle(executable: &str) -> Option<PathBuf> {
    let path = Path::new(executable);
    if !path.is_absolute() {
        return None;
    }
    let mut bundle = None;
    let mut prefix = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir | Component::CurDir) {
            return None;
        }
        prefix.push(component);
        if bundle.is_none() && component.as_os_str().to_str()?.ends_with(".app") {
            bundle = Some(prefix.clone());
        }
    }
    bundle
}

/// The same largest-footprint root used by status's `root_pid`.
pub fn workload_root(snap: &Snapshot, wi: usize) -> Option<&Proc> {
    snap.workloads[wi]
        .roots
        .iter()
        .filter_map(|pid| snap.proc_by_pid(*pid))
        .max_by_key(|p| p.fp)
}

/// Existing outer bundle for the workload's root; every returned path is redacted.
pub fn workload_bundle(snap: &Snapshot, wi: usize) -> Option<String> {
    let root = workload_root(snap, wi)?;
    let bundle = outer_bundle(&root.exe)
        .or_else(|| verified_clone(snap, root).map(|app| PathBuf::from(app.bundle)))?;
    let path = bundle.to_str()?;
    if !snap.is_local_path(path) || !bundle.is_dir() {
        return None;
    }
    let shown = crate::redact::display(path);
    (shown == path).then_some(shown)
}

/// Installed app identity supported by the kernel's clone path and bundle metadata.
#[derive(Clone)]
pub(crate) struct CloneApp {
    pub name: String,
    pub bundle: String,
    pub bundle_id: String,
}

// Parse only the macOS clone layout, never a command argument containing an app name.
fn clone_identity(executable: &str) -> Option<(&str, &str)> {
    let tail = executable
        .strip_prefix("/private/var/folders/")
        .or_else(|| executable.strip_prefix("/var/folders/"))?;
    let parts: Vec<_> = tail.split('/').collect();
    if parts.len() < 8 || parts[2] != "X" || !parts[4].starts_with("code_sign_clone.") {
        return None;
    }
    let id = parts[3].strip_suffix(".code_sign_clone")?;
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
    {
        return None;
    }
    let app_start = executable.find(&format!("/{}/", parts[5]))? + 1;
    let relative = &executable[app_start..];
    let (app, rest) = relative.split_once(".app.bundle/")?;
    if app.contains('/') || !rest.starts_with("Contents/MacOS/") || rest.contains("/../") {
        return None;
    }
    Some((id, relative))
}

/// Command-path evidence is corroborated by both bundle ID and executable identity.
/// It is weaker than a direct kernel app path, so attribution uses medium confidence.
pub(crate) fn verified_clone(snap: &Snapshot, p: &Proc) -> Option<CloneApp> {
    let (id, clone_relative) = clone_identity(&p.exe)?;
    let command = p.argv.first()?;
    let bundle = outer_bundle(command)?;
    let path = bundle.to_str()?;
    if !snap.is_local_path(path) || !bundle.is_dir() || !Path::new(command).is_file() {
        return None;
    }
    let name = bundle.file_stem()?.to_str()?;
    let executable = Path::new(command).strip_prefix(&bundle).ok()?.to_str()?;
    let cloned = format!("{name}.app.bundle/{executable}");
    if clone_relative != cloned {
        return None;
    }
    let raw = crate::collect::read_head_bytes(&bundle.join("Contents/Info.plist"), 256 * 1024)?;
    let plist::Value::Dictionary(info) =
        plist::Value::from_reader(std::io::Cursor::new(raw)).ok()?
    else {
        return None;
    };
    let declared_id = info.get("CFBundleIdentifier")?.as_string()?;
    let binary = info.get("CFBundleExecutable")?.as_string()?;
    if declared_id != id || executable != format!("Contents/MacOS/{binary}") {
        return None;
    }
    Some(CloneApp {
        name: name.into(),
        bundle: path.into(),
        bundle_id: id.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outer_bundle_ignores_nested_helpers_and_rejects_non_apps() {
        assert_eq!(
            outer_bundle(
                "/Applications/Chrome.app/Contents/Helpers/Renderer.app/Contents/MacOS/Renderer"
            ),
            Some(PathBuf::from("/Applications/Chrome.app"))
        );
        for path in [
            "/usr/bin/node",
            "relative/Example.app/bin",
            "/tmp/Example.app.bundle/bin",
            "/tmp/Example.app/../bin",
        ] {
            assert!(outer_bundle(path).is_none(), "{path}");
        }
    }
}
