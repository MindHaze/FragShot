#![cfg(windows)]

use std::{
    fs,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Output},
};

struct BuildFixture {
    _temp: tempfile::TempDir,
    project: PathBuf,
    tools: PathBuf,
    outside: PathBuf,
}

impl BuildFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project with spaces (test)");
        let tools = temp.path().join("fake toolchain");
        let outside = temp.path().join("different working directory");
        for path in [&project, &tools, &outside] {
            fs::create_dir(path).unwrap();
        }
        fs::write(
            project.join("builder.bat"),
            include_bytes!("../builder.bat"),
        )
        .unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "# Test project; only fake Cargo must run.\n",
        )
        .unwrap();
        fs::write(
            tools.join("rustc.cmd"),
            "@echo off\r\necho host: x86_64-pc-windows-msvc\r\nexit /b 0\r\n",
        )
        .unwrap();
        Self {
            _temp: temp,
            project,
            tools,
            outside,
        }
    }

    fn run(&self, fake_cargo: &str) -> Output {
        fs::write(self.tools.join("cargo.cmd"), fake_cargo).unwrap();
        let system_root = std::env::var_os("SystemRoot").unwrap();
        let system_root = PathBuf::from(system_root);
        let path = std::env::join_paths([
            self.tools.clone(),
            system_root.join("System32"),
            system_root.clone(),
        ])
        .unwrap();
        Command::new(system_root.join("System32").join("cmd.exe"))
            .args(["/D", "/S", "/C"])
            .raw_arg(format!(
                "call \"{}\"",
                self.project.join("builder.bat").display()
            ))
            .current_dir(&self.outside)
            .env("PATH", path)
            .env("CARGO_BUILD_TARGET", "aarch64-unknown-linux-gnu")
            .env("CARGO_TARGET_DIR", self.outside.join("unrelated target"))
            .output()
            .unwrap()
    }
}

const SUCCESS: &str = concat!(
    "@echo off\r\n",
    "if not exist Cargo.toml exit /b 20\r\n",
    "if not \"%~1\"==\"build\" exit /b 21\r\n",
    "if not \"%~2\"==\"--release\" exit /b 22\r\n",
    "if not \"%~3\"==\"--locked\" exit /b 23\r\n",
    "if not \"%~4\"==\"--target\" exit /b 24\r\n",
    "if not \"%~5\"==\"x86_64-pc-windows-msvc\" exit /b 25\r\n",
    "if not \"%~6\"==\"--target-dir\" exit /b 26\r\n",
    "if not \"%~7\"==\"%CD%\\target\" exit /b 27\r\n",
    "if not exist \"%~7\\%~5\\release\" mkdir \"%~7\\%~5\\release\"\r\n",
    ">\"%~7\\%~5\\release\\fragshot.exe\" echo fresh-build\r\n",
    "exit /b 0\r\n",
);

#[test]
fn builder_builds_native_release_and_copies_it_from_another_directory() {
    let fixture = BuildFixture::new();
    fs::write(fixture.project.join("fragshot.exe"), "previous build").unwrap();
    let output = fixture.run(SUCCESS);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(fixture.project.join("fragshot.exe")).unwrap(),
        b"fresh-build\r\n"
    );
    assert!(!fixture.outside.join("fragshot.exe").exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Build complete"));
}

#[test]
fn builder_propagates_build_failure_without_replacing_previous_executable() {
    let fixture = BuildFixture::new();
    let root_exe = fixture.project.join("fragshot.exe");
    fs::write(&root_exe, "previous build").unwrap();
    let stale = fixture
        .project
        .join("target/x86_64-pc-windows-msvc/release");
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("fragshot.exe"), "stale build").unwrap();
    let output = fixture.run("@echo off\r\nexit /b 7\r\n");
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(root_exe).unwrap(), "previous build");
    assert!(String::from_utf8_lossy(&output.stdout).contains("Build failed"));
}

#[test]
fn builder_rejects_success_without_an_executable() {
    let fixture = BuildFixture::new();
    let output = fixture.run("@echo off\r\nexit /b 0\r\n");
    assert!(!output.status.success());
    assert!(!fixture.project.join("fragshot.exe").exists());
}

#[test]
fn builder_rejects_a_directory_at_the_executable_destination() {
    let fixture = BuildFixture::new();
    fs::create_dir(fixture.project.join("fragshot.exe")).unwrap();
    let output = fixture.run(SUCCESS);
    assert!(!output.status.success());
    assert!(!fixture.project.join("fragshot.exe/fragshot.exe").exists());
}
