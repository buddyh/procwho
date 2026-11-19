//! Stock-shell regression for the copy/paste install path; no real installation.
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn shell_blocks(text: &str) -> Vec<&str> {
    text.split("```sh\n")
        .skip(1)
        .map(|s| s.split("```").next().unwrap())
        .collect()
}

#[test]
fn documented_shell_blocks_parse() {
    for file in [
        "README.md",
        "docs/CLI.md",
        "docs/AGENTS.md",
        "CONTRIBUTING.md",
    ] {
        let text = std::fs::read_to_string(root().join(file)).unwrap();
        for block in shell_blocks(&text) {
            assert!(
                Command::new("/bin/zsh")
                    .args(["-fn", "-c", block])
                    .status()
                    .unwrap()
                    .success(),
                "{file}: {block}"
            );
        }
    }
}

#[test]
fn readme_install_works_with_stock_path_and_fresh_home() {
    let dir = std::env::temp_dir().join(format!("procwho-doc-install-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("home")).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_procwho"), dir.join("procwho")).unwrap();
    assert!(Command::new("/usr/bin/tar")
        .args(["-czf", "procwho-0.1.0-macos-universal.tar.gz", "procwho"])
        .current_dir(&dir)
        .status()
        .unwrap()
        .success());
    let readme = std::fs::read_to_string(root().join("README.md")).unwrap();
    // The manual download block; the README leads with Homebrew, which this test cannot run.
    let block = shell_blocks(&readme)
        .into_iter()
        .find(|b| b.contains("tar -xzf"))
        .expect("README has a manual download block");
    let out = Command::new("/bin/zsh")
        .env_clear()
        .env("HOME", dir.join("home"))
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .args(["-ef", "-c", block])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("procwho 0.1.0"));
    assert!(dir.join("home/.local/bin/procwho").is_file());
    std::fs::remove_dir_all(dir).unwrap();
}
