#![cfg(unix)]

use assert_fs::TempDir;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn project() -> Result<TempDir, Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    std::fs::write(dir.join("install.sh"), include_str!("../install.sh"))?;
    std::fs::write(dir.join("release.sh"), include_str!("../release.sh"))?;
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"dufs\"\nversion = \"0.47.0\"\nedition = \"2021\"\n",
    )?;
    std::fs::create_dir(dir.join("src"))?;
    std::fs::write(
        dir.join("src/main.rs"),
        "fn main() { println!(\"dufs {}\", env!(\"CARGO_PKG_VERSION\")); }\n",
    )?;
    Ok(dir)
}

fn binary(path: &Path, version: &str) -> TestResult {
    std::fs::write(path, format!("#!/bin/sh\nprintf 'dufs {version}\\n'\n"))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn command_with_input(mut cmd: Command, input: &str) -> Result<Output, Box<dyn std::error::Error>> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(input.as_bytes())?;
    Ok(child.wait_with_output()?)
}

fn install(
    dir: &TempDir,
    filename: Option<&str>,
    input: &str,
) -> Result<Output, Box<dyn std::error::Error>> {
    let mut cmd = Command::new("sh");
    cmd.arg(dir.join("install.sh"))
        .arg(dir.join("install destination"));
    if let Some(filename) = filename {
        cmd.arg(filename);
    }
    command_with_input(cmd, input)
}

fn release(dir: &TempDir, args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
    Ok(Command::new("sh")
        .arg(dir.join("release.sh"))
        .args(args)
        .output()?)
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn installs_existing_release_with_custom_filename() -> TestResult {
    let dir = project()?;
    std::fs::create_dir_all(dir.join("target/release"))?;
    binary(&dir.join("target/release/dufs"), "0.47.1")?;

    let output = install(&dir, Some("dufsa"), "")?;
    assert_success(&output);
    let installed = dir.join("install destination/dufsa");
    assert!(installed.exists());
    assert_ne!(
        std::fs::metadata(installed)?.permissions().mode() & 0o111,
        0
    );
    assert!(!dir.join("install destination/dufs").exists());
    Ok(())
}

#[test]
fn refuses_to_replace_same_version() -> TestResult {
    let dir = project()?;
    std::fs::create_dir_all(dir.join("target/release"))?;
    std::fs::create_dir_all(dir.join("install destination"))?;
    binary(&dir.join("target/release/dufs"), "0.47.0")?;
    let installed = dir.join("install destination/dufs");
    binary(&installed, "0.47.0")?;
    let before = std::fs::read(&installed)?;

    let output = install(&dir, None, "y\n")?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("same version")
            || String::from_utf8_lossy(&output.stderr).contains("already has version")
    );
    assert_eq!(std::fs::read(installed)?, before);
    Ok(())
}

#[test]
fn replacing_different_or_unknown_version_requires_confirmation() -> TestResult {
    let dir = project()?;
    std::fs::create_dir_all(dir.join("target/release"))?;
    std::fs::create_dir_all(dir.join("install destination"))?;
    binary(&dir.join("target/release/dufs"), "0.47.1")?;
    let installed = dir.join("install destination/dufs");
    binary(&installed, "0.47.0")?;
    let before = std::fs::read(&installed)?;
    let output = install(&dir, None, "n\n")?;
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&installed)?, before);
    let output = install(&dir, None, "y\n")?;
    assert_success(&output);
    assert_eq!(
        std::fs::read(&installed)?,
        std::fs::read(dir.join("target/release/dufs"))?
    );

    std::fs::write(&installed, b"unknown binary")?;
    let output = install(&dir, None, "")?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Cannot determine"));
    assert_eq!(std::fs::read(&installed)?, b"unknown binary");
    let output = install(&dir, None, "y\n")?;
    assert_success(&output);
    Ok(())
}

#[test]
fn rejects_symlink_and_invalid_filename() -> TestResult {
    let dir = project()?;
    std::fs::create_dir_all(dir.join("target/release"))?;
    std::fs::create_dir_all(dir.join("install destination"))?;
    binary(&dir.join("target/release/dufs"), "0.47.1")?;
    let target = dir.join("untouched");
    std::fs::write(&target, b"untouched")?;
    std::os::unix::fs::symlink(&target, dir.join("install destination/dufs"))?;
    assert!(!install(&dir, None, "y\n")?.status.success());
    assert_eq!(std::fs::read(target)?, b"untouched");
    for name in ["", "../dufs", ".", ".."] {
        assert!(!install(&dir, Some(name), "")?.status.success());
    }
    Ok(())
}

#[test]
fn rejects_release_with_unrecognized_version() -> TestResult {
    let dir = project()?;
    std::fs::create_dir_all(dir.join("target/release"))?;
    binary(&dir.join("target/release/dufs"), "unknown")?;
    let output = install(&dir, None, "")?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Unrecognized local release version"));
    assert!(!dir.join("install destination/dufs").exists());
    Ok(())
}

#[test]
fn declining_missing_release_cancels_even_if_debug_exists() -> TestResult {
    let dir = project()?;
    std::fs::create_dir_all(dir.join("target/debug"))?;
    binary(&dir.join("target/debug/dufs"), "0.47.0")?;
    let output = install(&dir, None, "n\n")?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Installation cancelled"));
    assert!(!dir.join("install destination/dufs").exists());
    Ok(())
}

#[test]
fn rejects_empty_install_directory() -> TestResult {
    let dir = project()?;
    let output = Command::new("sh")
        .arg(dir.join("install.sh"))
        .arg("")
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be empty"));
    Ok(())
}

#[test]
fn release_builds_bump_versions_and_update_lockfile() -> TestResult {
    let dir = project()?;
    for (args, version) in [
        (vec![], "0.47.1"),
        (vec!["--bump", "minor"], "0.48.0"),
        (vec!["--bump", "major"], "1.0.0"),
        (vec!["--version", "2.3.4"], "2.3.4"),
    ] {
        let output = release(&dir, &args)?;
        assert_success(&output);
        assert!(std::fs::read_to_string(dir.join("Cargo.toml"))?
            .contains(&format!("version = \"{version}\"")));
        assert!(std::fs::read_to_string(dir.join("Cargo.lock"))?
            .contains(&format!("version = \"{version}\"")));
        let output = Command::new(dir.join("target/release/dufs"))
            .arg("--version")
            .output()?;
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            format!("dufs {version}")
        );
        assert!(Command::new("cargo")
            .arg("pkgid")
            .arg("--locked")
            .arg("--offline")
            .arg("--manifest-path")
            .arg(dir.join("Cargo.toml"))
            .output()?
            .status
            .success());
    }
    for args in [
        vec!["--version", "2.3.4"],
        vec!["--version", "2.3.3"],
        vec!["--version", "03.0.0"],
        vec!["--version", "3.0"],
        vec!["--bump", "invalid"],
    ] {
        assert!(!release(&dir, &args)?.status.success());
        assert!(std::fs::read_to_string(dir.join("Cargo.toml"))?.contains("version = \"2.3.4\""));
    }
    Ok(())
}

#[test]
fn accepting_missing_release_builds_and_installs_it() -> TestResult {
    let dir = project()?;
    let output = install(&dir, None, "y\n")?;
    assert_success(&output);
    let output = Command::new(dir.join("install destination/dufs"))
        .arg("--version")
        .output()?;
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "dufs 0.47.1"
    );
    Ok(())
}
