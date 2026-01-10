use anyhow::*;
use clap::{Parser, Subcommand, ValueEnum};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

//
// ---------------- CLI ----------------
//

#[derive(Parser)]
#[command(author, version, about = "IonOS build orchestration (xtask)")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,

    /// Target architecture (e.g. x86_64, aarch64)
    #[arg(long, default_value = "x86_64", global = true)]
    arch: String,

    /// Virtual machine backend
    #[arg(long, value_enum, default_value = "qemu", global = true)]
    vm: Vm,
}

#[derive(Subcommand)]
enum Cmd {
    Build,
    Test,
    Run,
}

#[derive(ValueEnum, Clone)]
enum Vm {
    Qemu,
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Cmd::Build => build(&cli, false),
        Cmd::Test => {
            build(&cli, true)?;
            run_vm(&cli)
        },
        Cmd::Run => {
            build(&cli, false)?;
            run_vm(&cli)
        }
    }
}

//
// ---------------- build ----------------
//

fn build(cli: &Cli, test: bool) -> Result<()> {
    let arch = &cli.arch;

    let mut objects = Vec::<PathBuf>::new();

    let build_root = format!("build/{arch}");
    let dist_root = format!("dist/{arch}");
    let target_json = format!("targets/{arch}.json");

    // --------------------------------------------------
    // ASM: app/src/<arch>/**/*.asm
    // --------------------------------------------------
    for asm in find_files(format!("app/src/{arch}"), "asm")? {
        let out = mirror_path(&asm, &format!("app/src/{arch}"), &build_root)
            .with_extension("o");

        fs::create_dir_all(out.parent().unwrap())?;

        docker(&[
            "nasm",
            "-f", "elf64",
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
        let out = mirror_path(&c, "app/src/kernel", &format!("{build_root}/kernel"))
            .with_extension("o");

        fs::create_dir_all(out.parent().unwrap())?;

        docker(&[
            "x86_64-elf-gcc",
            "-c",
            "-ffreestanding",
            "-m64",
            "-mno-red-zone",
            "-fno-omit-frame-pointer",
            "-fno-pic",
            "-I", "app/src/kernel/c_entry",
            c.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])?;

        objects.push(out);
    }

    // --------------------------------------------------
    // Rust kernel (staticlib)
    // --------------------------------------------------

    let target_dir = format!("{build_root}/kernel/rust");

    let mut rust_cmd = vec![
        "cargo",
        "+nightly",
        "build",
        "-p", "ion-kernel",
        "--target", &target_json,
        "-Z", "build-std=core,alloc",
        "-Z", "build-std-features=compiler-builtins-mem",
        "--target-dir", &target_dir,
    ];

    if test {
        rust_cmd.extend(["--features", "test"]);
    } else {
        rust_cmd.extend(["--no-default-features"]);
    }

    docker(&rust_cmd)?;

    // NOTE: target naming scheme => <arch>.json → <arch>/debug
    let rust_lib = PathBuf::from(format!(
        "{build_root}/kernel/rust/{arch}/debug/libion_kernel.a"
    ));

    if !rust_lib.exists() {
        bail!("Rust staticlib not found: {}", rust_lib.display());
    }

    objects.push(rust_lib);

    // --------------------------------------------------
    // Link
    // --------------------------------------------------
    fs::create_dir_all(&dist_root)?;

    let elf = format!("{dist_root}/kernel.elf");

    let mut ld_args = vec![
        "x86_64-elf-ld",
        "-n",
        "--gc-sections",
        "-T", "app/targets/x86_64/linker.ld",
        "-o", &elf,
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
        "-O", "binary",
        &elf,
        &elf.replace(".elf", ".bin"),
    ])?;

    // --------------------------------------------------
    // ISO
    // --------------------------------------------------
    fs::copy(&elf, "app/targets/x86_64/iso/boot/kernel.elf")?;

    let iso = format!("{dist_root}/kernel.iso");

    docker(&[
        "grub-mkrescue",
        "/usr/lib/grub/i386-pc",
        "-o", &iso,
        "app/targets/x86_64/iso",
    ])?;

    Ok(())
}

//
// ---------------- run ----------------
//

fn run_vm(cli: &Cli) -> Result<()> {
    let iso = format!("dist/{}/kernel.iso", cli.arch);

    match cli.vm {
        Vm::Qemu => run_qemu(&iso),
        Vm::Bochs => bail!("bochs backend not implemented yet"),
    }
}

fn run_qemu(iso: &str) -> Result<()> {
    let mut binding = Command::new("qemu-system-x86_64");
    let cmd = binding
        .args([
            iso,
            "-debugcon", "stdio",
            "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        ]);

    let status = cmd.status()?.code();
    if status != Some(33) { // qemu exit code for `OK`
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
