use anyhow::*;
use clap::{Parser, Subcommand, ValueEnum};
use std::{
    fs,
    io::stdin,
    path::{Path, PathBuf},
    process::Command,
};

//
// ---------------- CLI ----------------
//

#[derive(Parser)]
#[command(
    author = "Mostafa Khaled",
    version,
    about = "IonOS build orchestration (xtask)"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,

    /// Target architecture (e.g. x86_64, aarch64)
    #[arg(long, default_value = "x86_64", global = true)]
    arch: String,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build an ISO for Ion OS
    Build {
        /// Wether or not to optimize Rust's build.
        #[arg(long)]
        release: bool,
        /// Wether or not to build for tests
        #[arg(long)]
        test: bool,
    },
    /// Build and Run tests
    Test {
        /// Sets the virtual machine to run the tests.
        #[arg(long, value_enum, default_value = "qemu", global = true)]
        vm: Vm,
    },
    /// Run the output ISO.
    Run {
        /// Wether or not build.
        #[arg(long, name = "no-build")]
        no_build: bool,
        /// Sets the virtual machine to run the ISO.
        #[arg(long, value_enum, default_value = "qemu", global = true)]
        vm: Vm,
        /// Wether or not to optimize Rust's build.
        #[arg(long)]
        release: bool,
    },

    /// Run checks on tools, project, and git.
    Doctor {
        /// Wether or not to check more components (eg. clippy, rustfmt...)
        #[arg(long)]
        minimal: bool,
        /// Wether or not to print logs while checking
        #[arg(long)]
        verbose: bool,
        /// Virtual machine to check.
        #[arg(long)]
        vm: Option<Vm>,
    },
}

#[derive(ValueEnum, Clone, Copy)]
enum Vm {
    /// qemu-system-{arch}
    Qemu,
    /// Bochs, unimplemented.
    Bochs,
}

//
// ---------------- helpers ----------------
//

fn sh(cmd: &mut Command) -> Result<()> {
    let status = cmd.status()?;
    if !status.success() {
        bail!("command failed: {:?}", cmd);
    }
    Ok(())
}

fn docker(cmd: &[&str]) -> Result<()> {
    if cmd.is_empty() {
        bail!("docker(): empty command");
    }

    let pwd = std::env::current_dir()?;

    let mut c = Command::new("docker");
    c.args([
        "run",
        "--rm",
        "-t",
        "-v",
        &format!("{}:/root/env", pwd.display()),
        "-w",
        "/root/env",
        "ionicmc/ion-os-buildenv",
    ]);

    c.args(cmd);
    sh(&mut c)
}

//
// ---------------- entry ----------------
//

const SUPPORTED_ARCHS: &[&str] = &["x86_64"];

fn main() -> Result<()> {
    let cli = Cli::parse();

    if !SUPPORTED_ARCHS.contains(&cli.arch.as_str()) {
        bail!(
            "unsupported architecture: {}\n\nThe supported architectures are: \n{}",
            cli.arch,
            SUPPORTED_ARCHS.join("\n")
        )
    }

    match cli.command {
        Cmd::Build { release, test } => build(&cli, test, release),
        Cmd::Test { vm } => {
            build(&cli, true, false)?;
            println!(
                "build complete, you may need to change the permissions of kernel.iso before executing."
            );
            run_vm(vm, cli.arch, true)
        }
        Cmd::Run {
            no_build,
            vm,
            release,
        } => {
            if !no_build {
                build(&cli, false, release)?;
            }
            println!(
                "build complete, you may need to change the permissions of kernel.iso before executing."
            );
            run_vm(vm, cli.arch, false)
        }
        Cmd::Doctor {
            minimal,
            vm,
            verbose,
        } => doctor(minimal, vm, verbose),
    }
}

//
// ---------------- build ----------------
//

fn build(cli: &Cli, test: bool, release: bool) -> Result<()> {
    if test && release {
        bail!("internal error: cannot build with both `test` and `release`");
    }

    let arch = &cli.arch;

    let mut objects = Vec::<PathBuf>::new();

    let build_root = format!("build/{arch}");
    let dist_root = format!("dist/{arch}");
    let target_json = format!("targets/{arch}.json");

    // --------------------------------------------------
    // ASM: app/src/<arch>/**/*.asm
    // --------------------------------------------------
    for asm in find_files(format!("app/src/{arch}"), "asm")? {
        let out = mirror_path(&asm, &format!("app/src/{arch}"), &build_root).with_extension("o");

        fs::create_dir_all(out.parent().unwrap())?;

        docker(&[
            "nasm",
            "-f",
            "elf64",
            asm.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])?;

        objects.push(out);
    }

    // --------------------------------------------------
    // C: app/src/kernel/**/*.c
    // --------------------------------------------------
    for c in find_files("app/src/kernel", "c")? {
        let out =
            mirror_path(&c, "app/src/kernel", &format!("{build_root}/kernel")).with_extension("o");

        fs::create_dir_all(out.parent().unwrap())?;

        docker(&[
            "x86_64-elf-gcc",
            "-c",
            "-ffreestanding",
            "-m64",
            "-mno-red-zone",
            "-fno-omit-frame-pointer",
            "-fno-pic",
            "-I",
            "app/src/kernel/c_entry",
            c.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])?;

        objects.push(out);
    }

    // --------------------------------------------------
    // Rust kernel (staticlib)
    // --------------------------------------------------

    let mut target_dir = format!("{build_root}/kernel/rust");

    if test {
        target_dir.push_str("-tests");
    }

    let mut rust_cmd = vec![
        "cargo",
        "+nightly",
        "build",
        "-p",
        "ion-kernel",
        "--target",
        &target_json,
        "-Z",
        "build-std=core,alloc",
        "-Z",
        "build-std-features=compiler-builtins-mem",
        "--target-dir",
        &target_dir,
    ];

    if test {
        rust_cmd.extend(["--features", "test"]);
    } else {
        rust_cmd.extend(["--no-default-features"]);
    }

    if release {
        rust_cmd.push("--release");
    }

    docker(&rust_cmd)?;

    // NOTE: target naming scheme => <arch>.json → <arch>/debug
    let rust_lib = PathBuf::from(format!("{target_dir}/{arch}/debug/libion_kernel.a"));

    if !rust_lib.exists() {
        bail!("Rust staticlib not found: {}", rust_lib.display());
    }

    objects.push(rust_lib);

    // --------------------------------------------------
    // Link
    // --------------------------------------------------
    fs::create_dir_all(&dist_root)?;

    let elf = if test {
        format!("{dist_root}/test/kernel.elf")
    } else {
        format!("{dist_root}/kernel.elf")
    };

    let mut ld_args = vec![
        "x86_64-elf-ld",
        "-n",
        "--gc-sections",
        "-T",
        "app/targets/x86_64/linker.ld",
        "-o",
        &elf,
    ];

    for obj in &objects {
        ld_args.push(obj.to_str().ok_or_else(|| anyhow!("invalid UTF-8 path"))?);
    }

    docker(&ld_args)?;

    // --------------------------------------------------
    // Objcopy
    // --------------------------------------------------
    docker(&[
        "objcopy",
        "-O",
        "binary",
        &elf,
        &elf.replace(".elf", ".bin"),
    ])?;

    // --------------------------------------------------
    // ISO
    // --------------------------------------------------
    fs::copy(&elf, "app/targets/x86_64/iso/boot/kernel.elf")?;

    let iso = if test {
        format!("{dist_root}/test/ion_os_{arch}_test.iso")
    } else {
        format!("{dist_root}/ion_os_{arch}.iso")
    };

    docker(&[
        "grub-mkrescue",
        "/usr/lib/grub/i386-pc",
        "-o",
        &iso,
        "app/targets/x86_64/iso",
    ])?;

    Ok(())
}

//
// ---------------- run ----------------
//

fn run_vm(vm: Vm, arch: String, test: bool) -> Result<()> {
    let mut iso = format!("dist/{}/", arch);

    if test {
        iso.push_str(&format!("test/ion_os_{arch}_test.iso"));
    } else {
        iso.push_str(&format!("ion_os_{arch}.iso"));
    }

    match vm {
        Vm::Qemu => run_qemu(&iso),
        Vm::Bochs => bail!("bochs backend not implemented yet"),
    }
}

fn run_qemu(iso: &str) -> Result<()> {
    let mut binding = Command::new("qemu-system-x86_64");
    let cmd = binding.args([
        iso,
        "-debugcon",
        "stdio",
        "-device",
        "isa-debug-exit,iobase=0xf4,iosize=0x04",
    ]);

    let status = cmd.status()?.code();
    if status != Some(33) {
        // qemu exit code for `OK`
        bail!("qemu failed: {:?}", cmd);
    }
    Ok(())
}

//
// ---------------- fs helpers ----------------
//

fn find_files(root: impl AsRef<Path>, ext: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(root.as_ref(), ext, &mut out)?;
    Ok(out)
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            walk(&path, ext, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(path);
        }
    }
    Ok(())
}

fn mirror_path(path: &Path, from: &str, to: &str) -> PathBuf {
    Path::new(to).join(path.strip_prefix(from).unwrap())
}

// doctor

use anyhow::{Context, Result};

fn cmd_exists(cmd: &str) -> bool {
    Command::new(cmd).arg("--version").output().is_ok()
}

fn doctor(minimal: bool, vm: Option<Vm>, verbose: bool) -> Result<()> {
    check_rust(minimal, verbose)?;
    check_tools(minimal, vm, verbose)?;
    check_git(verbose)?;

    if !minimal {
        check_cleanliness(verbose)?
    }

    check_docker_daemon(verbose)?;

    check_project(verbose)?;

    println!("\n\ndoctor: scan passed.");

    Ok(())
}

fn check_tools(minimal: bool, vm: Option<Vm>, verbose: bool) -> Result<()> {
    println!("- External tools");

    // vm first

    let vm_cmd = if let Some(vm) = vm {
        match vm {
            Vm::Qemu => {
                if verbose {
                    println!("checking for qemu archs: x86_64");
                }
                ["qemu-system-x86_64"]
            }
            Vm::Bochs => bail!("bochs is not supported currently."),
        }
    } else {
        println!("Please select a virtual machine to check: ");
        println!();
        if minimal {
            println!("1. Qemu (x86_64)");
        } else {
            println!("1. Qemu (all architectures)");
        }
        println!("[press a number to continue]");
        let mut buf = String::new();
        stdin().read_line(&mut buf)?;

        let vm_cmd = match buf.as_str().trim() {
            "1" => {
                if minimal {
                    ["qemu-system-x86_64"]
                } else {
                    ["qemu-system-x86_64"] // TODO: add more archs.
                }
            }
            v => bail!("unknown option: {v}"),
        };

        vm_cmd
    };

    let mut tools = vec![];

    tools.extend(vm_cmd);

    for tool in tools {
        if verbose {
            println!("checking tool: {tool}");
        }
        if !cmd_exists(tool) {
            anyhow::bail!(
                "missing tool: {tool}\n\nhint: you can usually find missing tools in your Operating System's package manager (apt, pacman, winget, etc.)"
            );
        }
    }

    Ok(())
}

fn check_git(verbose: bool) -> Result<()> {
    println!("- Git state");

    if verbose {
        println!("checking git.");
    }

    sh(Command::new("git").arg("--version")).context("git not found")?;

    if verbose {
        println!("checking `git status`");
    }

    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()?;

    if !output.stdout.is_empty() {
        println!(
            "  ⚠️ working tree not clean\n\n  hint: run `git add .` to stage all changes, and `git commit -m \"<MESSAGE>\"` to commit changes."
        );
    }

    Ok(())
}

fn check_project(verbose: bool) -> Result<()> {
    println!("- Project checks");

    if verbose {
        println!("checking cargo metadata")
    }

    sh(Command::new("cargo").args(["metadata", "--no-deps"])).context("cargo metadata failed")?;

    if verbose {
        println!("running `cargo check`");
    }

    // TODO: add `--deep` option to change this to `clippy`

    sh(Command::new("cargo").args(["check"]))?;

    Ok(())
}

fn check_cleanliness(verbose: bool) -> Result<()> {
    println!("- Rust components");

    let components = ["rustfmt", "clippy", "llvm-tools"];

    let output = Command::new("rustup")
        .args(["component", "list", "--installed"])
        .output()
        .context("failed to query rustup components")?;

    let installed = String::from_utf8_lossy(&output.stdout);

    for c in components {
        if verbose {
            println!("checking rust tool: {c}");
        }
        if !installed.contains(c) {
            anyhow::bail!(
                "missing rustup component: {c}.\n\nhint: install this tool via `rustup component add {c}`\n\nIf you do not need `{c}`, you can silence this error (and all other non-essential tool errors) via `--minimal`"
            );
        }
    }

    Ok(())
}

fn check_rust(_: bool, verbose: bool) -> Result<()> {
    println!("- Rust toolchain");

    if verbose {
        println!("checking: rustup")
    }

    if !cmd_exists("rustup") {
        bail!(
            "rustup was not found.\n\nhint: You can install it via `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`"
        );
    }
    if verbose {
        println!("checking: rustc")
    }
    if !cmd_exists("rustc") {
        bail!(
            "The rust compiler was not found.\n\nhint: You can install it via rustup: `rustup toolchain install nightly`, `rustup default nightly`"
        );
    }
    if verbose {
        println!("checking: cargo")
    }
    if !cmd_exists("cargo") {
        bail!(
            "Cargo was not found\n\nhint: You can install it via rustup: `rustup toolchain install nightly`, `rustup default nightly`"
        );
    }

    Ok(())
}

fn check_docker_daemon(verbose: bool) -> Result<()> {
    println!("• Docker daemon");

    if verbose {
        println!("querying docker daemon");
    }

    let status = Command::new("docker")
        .arg("info")
        .status()
        .context("docker not installed")?;

    if verbose {
        println!("checking exit status")
    }

    if !status.success() {
        anyhow::bail!(
            "docker daemon not running\n\
             hint: start docker via `systemctl start docker`, or Docker Desktop"
        );
    }

    Ok(())
}
