use serde::Serialize;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Debug)]
pub(crate) struct BuildSystemSmokeOptions {
    pub(crate) make: bool,
    pub(crate) cmake: bool,
}

#[derive(Debug, Serialize)]
struct SmokeReport {
    schema_version: u32,
    zero_config_backend_override: bool,
    make: Option<SmokeResult>,
    cmake: Option<SmokeResult>,
    verdict: String,
}

#[derive(Debug, Serialize)]
struct SmokeResult {
    command_count: usize,
    exit_codes: Vec<i32>,
    object_created: bool,
    executable_created: bool,
}

pub(crate) fn run_build_system_smoke(
    options: BuildSystemSmokeOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    if !options.make && !options.cmake {
        return Err("select --make and/or --cmake".into());
    }
    let wrapper = wrapper_binary()?;
    let make = options.make.then(|| make_smoke(&wrapper)).transpose()?;
    let cmake = options.cmake.then(|| cmake_smoke(&wrapper)).transpose()?;
    let report = SmokeReport {
        schema_version: 1,
        zero_config_backend_override: true,
        make,
        cmake,
        verdict: "pass".into(),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn wrapper_binary() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let executable = std::env::current_exe()?;
    let wrapper = executable
        .parent()
        .ok_or("xtask executable has no parent")?
        .join("gcc-formed");
    if !wrapper.is_file() {
        return Err(format!("build wrapper first: missing {}", wrapper.display()).into());
    }
    Ok(wrapper)
}

fn clean_command(program: impl AsRef<Path>) -> Command {
    let mut command = Command::new(program.as_ref());
    command.env_remove("FORMED_BACKEND_GCC");
    command.env_remove("FORMED_BACKEND_LAUNCHER");
    command
}

fn checked(output: Output, label: &str) -> Result<Output, Box<dyn std::error::Error>> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "{label} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

fn make_smoke(wrapper: &Path) -> Result<SmokeResult, Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    fs::write(temp.path().join("main.c"), "int main(void) { return 0; }\n")?;
    fs::write(
        temp.path().join("Makefile"),
        "all: app\napp: main.o\n\t$(CC) main.o -o app\nmain.o: main.c\n\t$(CC) -MMD -MP -c main.c -o main.o\n",
    )?;
    let output = checked(
        clean_command("make")
            .arg("-j2")
            .arg(format!("CC={}", wrapper.display()))
            .current_dir(temp.path())
            .output()?,
        "make smoke",
    )?;
    Ok(SmokeResult {
        command_count: 2,
        exit_codes: vec![output.status.code().unwrap_or(128)],
        object_created: temp.path().join("main.o").is_file()
            && temp.path().join("main.d").is_file(),
        executable_created: temp.path().join("app").is_file(),
    })
}

fn cmake_smoke(wrapper: &Path) -> Result<SmokeResult, Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    #[cfg(unix)]
    let compiler = {
        use std::os::unix::fs::symlink;
        let alias = temp.path().join("g++-formed");
        symlink(wrapper, &alias)?;
        alias
    };
    #[cfg(not(unix))]
    let compiler = wrapper.to_path_buf();
    fs::write(temp.path().join("main.cpp"), "int main() { return 0; }\n")?;
    fs::write(
        temp.path().join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.16)\nproject(formed_smoke LANGUAGES CXX)\nadd_executable(app main.cpp)\n",
    )?;
    let build = temp.path().join("build");
    let configure = checked(
        clean_command("cmake")
            .args(["-S", ".", "-B", "build"])
            .arg(format!("-DCMAKE_CXX_COMPILER={}", compiler.display()))
            .current_dir(temp.path())
            .output()?,
        "cmake configure",
    )?;
    let compile = checked(
        clean_command("cmake")
            .args(["--build", "build", "--parallel", "2"])
            .current_dir(temp.path())
            .output()?,
        "cmake build",
    )?;
    let object_created = walk_has_extension(&build, "o")?;
    Ok(SmokeResult {
        command_count: 2,
        exit_codes: vec![
            configure.status.code().unwrap_or(128),
            compile.status.code().unwrap_or(128),
        ],
        object_created,
        executable_created: build.join("app").is_file(),
    })
}

fn walk_has_extension(root: &Path, extension: &str) -> Result<bool, std::io::Error> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() && walk_has_extension(&path, extension)? {
            return Ok(true);
        }
        if path.extension() == Some(OsString::from(extension).as_os_str()) {
            return Ok(true);
        }
    }
    Ok(false)
}
