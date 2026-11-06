use anyhow::*;
use clap::{Parser, Subcommand, ValueEnum};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write, stdin},
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
        /// Target architecture (e.g. x86_64, aarch64)
        #[arg(long, default_value = "x86_64", global = true)]
        arch: String,
    },
    /// Build and Run tests
    Test {
        /// Sets the virtual machine to run the tests.
        #[arg(long, value_enum, default_value = "qemu", global = true)]
        vm: Vm,
        /// Wether or not build.
        #[arg(long, name = "no-build")]
        no_build: bool,
        /// Target architecture (e.g. x86_64, aarch64)
        #[arg(long, default_value = "x86_64", global = true)]
        arch: String,
        /// Firmware to use when running the disk image
        #[arg(long, value_enum, default_value = "bios")]
        firmware: Firmware,
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
        /// Target architecture (e.g. x86_64, aarch64)
        #[arg(long, default_value = "x86_64", global = true)]
        arch: String,
        /// Firmware to use when running the disk image
        #[arg(long, value_enum, default_value = "bios")]
        firmware: Firmware,
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

    /// Filesystem operations for IonFS
    Fs {
        #[command(subcommand)]
        subcommand: FsCmd,
    },
}

#[derive(Subcommand)]
enum FsCmd {
    /// Create a new IonFS filesystem
    Make {
        /// Output path for disk image
        #[arg(long, default_value = "dist/rootfs.img")]
        path: String,
        /// Create the disk image file
        #[arg(long, default_value_t = false)]
        create: bool,
        /// GPT partition index to format when `path` points to a whole-disk image
        #[arg(long)]
        partition: Option<u32>,
    },
    /// Read and display IonFS filesystem contents
    Tree {
        /// Path to disk image
        #[arg(long, default_value = "dist/rootfs.img")]
        path: String,
        /// Inode numbers to display contents for
        #[arg(long, value_name = "INODE")]
        read_inode: Vec<u64>,
        /// GPT partition index to inspect when `path` points to a whole-disk image
        #[arg(long)]
        partition: Option<u32>,
    },
    /// Port files from host to disk image
    Port {
        /// Path to disk image
        #[arg(long, default_value = "dist/rootfs.img")]
        path: String,
        /// Destination path in filesystem (default: @/)
        #[arg(long, default_value = "/")]
        dest: String,
        /// Source files to port (host paths)
        #[arg(value_name = "FILE")]
        files: Vec<PathBuf>,
        /// Recursively port directories maintaining structure. Required for: porting directories, using non-root destinations, or porting multiple files
        #[arg(short, long)]
        recursive: bool,
        /// GPT partition index to update when `path` points to a whole-disk image
        #[arg(long)]
        partition: Option<u32>,
    },
    /// Create a directory
    Mkdir {
        /// Path to disk image
        #[arg(long, default_value = "dist/rootfs.img")]
        path: String,
        /// Directory path in filesystem (e.g. @/foo or @/foo/bar)
        #[arg(value_name = "DIR")]
        dir: String,
        /// GPT partition index to update when `path` points to a whole-disk image
        #[arg(long)]
        partition: Option<u32>,
    },
    /// Create an empty file
    Touch {
        /// Path to disk image
        #[arg(long, default_value = "dist/rootfs.img")]
        path: String,
        /// File path in filesystem (e.g. @/foo.txt or @/dir/file.txt)
        #[arg(value_name = "FILE")]
        file: String,
        /// GPT partition index to update when `path` points to a whole-disk image
        #[arg(long)]
        partition: Option<u32>,
    },
    /// Remove a file or empty directory
    Rm {
        /// Path to disk image
        #[arg(long, default_value = "dist/rootfs.img")]
        path: String,
        /// Path to remove (e.g. @/foo.txt or @/dir)
        #[arg(value_name = "PATH")]
        path_to_remove: String,
        /// GPT partition index to update when `path` points to a whole-disk image
        #[arg(long)]
        partition: Option<u32>,
    },
    /// List directory contents (like Unix ls command)
    Ls {
        /// Path to disk image
        #[arg(long, default_value = "dist/rootfs.img")]
        path: String,
        /// Directory to list (default: /)
        #[arg(default_value = "/")]
        dir: String,
        /// Long format (show inode numbers and sizes)
        #[arg(short, long)]
        long: bool,
        /// Show all entries (currently same as normal)
        #[arg(short, long)]
        all: bool,
        /// GPT partition index to inspect when `path` points to a whole-disk image
        #[arg(long)]
        partition: Option<u32>,
    },
    /// Quick view file contents
    Cat {
        /// Path to disk image
        #[arg(long, default_value = "dist/rootfs.img")]
        path: String,
        /// File path in filesystem (e.g. @/file.txt or @/dir/file.txt)
        #[arg(value_name = "FILE")]
        file: String,
        /// GPT partition index to inspect when `path` points to a whole-disk image
        #[arg(long)]
        partition: Option<u32>,
    },
}

#[derive(ValueEnum, Clone, Copy)]
enum Vm {
    /// qemu-system-{arch}
    Qemu,
    /// Bochs, unimplemented.
    Bochs,
}

#[derive(ValueEnum, Clone, Copy)]
enum Firmware {
    Bios,
    Uefi,
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

fn docker_with_options(cmd: &[&str], privileged: bool) -> Result<()> {
    if cmd.is_empty() {
        bail!("docker(): empty command");
    }

    let pwd = std::env::current_dir()?;

    let mut c = Command::new("docker");
    c.args(["run", "--rm", "-t"]);
    if privileged {
        c.arg("--privileged");
    }
    c.args([
        "-v",
        &format!("{}:/root/env", pwd.display()),
        "-w",
        "/root/env",
        "ionicmc/ion-os-buildenv",
    ]);

    c.args(cmd);
    sh(&mut c)?;
    Ok(())
}

fn docker(cmd: &[&str]) -> Result<()> {
    docker_with_options(cmd, false)
}

fn docker_privileged(cmd: &[&str]) -> Result<()> {
    docker_with_options(cmd, true)
}

//
// ---------------- entry ----------------
//

const SUPPORTED_ARCHS: &[&str] = &["x86_64"];

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Cmd::Build { release, test, .. } => build(&cli, test, release),
        Cmd::Test { vm , no_build, ref arch, firmware } => {
            if !SUPPORTED_ARCHS.contains(&arch.as_str()) {
                bail!(
                    "unsupported architecture: {}\n\nThe supported architectures are: \n{}",
                    arch,
                    SUPPORTED_ARCHS.join("\n")
                )
            }
            if !no_build {
                build(&cli, true, false)?;
                println!(
                    "build complete, you may need to change the permissions of kernel.iso before executing."
                );
            }
            run_vm(vm, arch.clone(), true, firmware)
        }
        Cmd::Run {
            no_build,
            vm,
            release,
            ref arch,
            firmware,
        } => {
            if !SUPPORTED_ARCHS.contains(&arch.as_str()) {
                bail!(
                    "unsupported architecture: {}\n\nThe supported architectures are: \n{}",
                    arch,
                    SUPPORTED_ARCHS.join("\n")
                )
            }
            if !no_build {
                build(&cli, false, release)?;
                println!(
                    "build complete, you may need to change the permissions of kernel.iso before executing."
                );
            }
            run_vm(vm, arch.clone(), false, firmware)
        }
        Cmd::Doctor {
            minimal,
            vm,
            verbose,
        } => doctor(minimal, vm, verbose),
        Cmd::Fs { subcommand } => match subcommand {
            FsCmd::Make { path, create, partition } => {
                let p = PathBuf::from(path);
                if create {
                    let f = fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&p)?;
                    f.set_len(64 * 1024 * 1024)?; // 64 MiB
                }
                fs_make(&p, partition)
            }
            FsCmd::Tree { path, read_inode, partition } => {
                let p = PathBuf::from(path);
                fs_read(&p, partition, &read_inode)
            }
            FsCmd::Port { path, dest, files, recursive, partition } => {
                let p = PathBuf::from(path);
                fs_port(&p, partition, &dest, &files, recursive)
            }
            FsCmd::Mkdir { path, dir, partition } => {
                let p = PathBuf::from(path);
                fs_mkdir(&p, partition, &dir)
            }
            FsCmd::Touch { path, file, partition } => {
                let p = PathBuf::from(path);
                fs_touch(&p, partition, &file)
            }
            FsCmd::Rm { path, path_to_remove, partition } => {
                let p = PathBuf::from(path);
                fs_rm(&p, partition, &path_to_remove)
            }
            FsCmd::Ls { path, dir, long, all, partition } => {
                let p = PathBuf::from(path);
                fs_ls(&p, partition, &dir, long, all)
            }
            FsCmd::Cat { path, file, partition } => {
                let p = PathBuf::from(path);
                fs_cat(&p, partition, &file)
            }
        },
    }
}

//
// ---------------- build ----------------
//

fn build(cli: &Cli, test: bool, release: bool) -> Result<()> {
    if test && release {
        bail!("internal error: cannot build with both `test` and `release`");
    }

    let (Cmd::Build { ref arch, .. } | Cmd::Run { ref arch, .. } | Cmd::Test { ref arch, .. }) = cli.command else { unreachable!() };
    let arch = arch.clone();

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
            "-Wa,--noexecstack",
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
        "json-target-spec",
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

    build_boot_disk(Path::new(&elf), Path::new(&dist_root), &arch, test)?;

    Ok(())
}

fn ensure_rootfs_image() -> Result<PathBuf> {
    let path = PathBuf::from("dist/rootfs.img");

    if path.exists() {
        return Ok(path);
    }

    fs::create_dir_all(path.parent().unwrap())?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    file.set_len(DEFAULT_ROOTFS_SIZE_BYTES)?;

    drop(file);
    fs_make(&path, None)?;
    Ok(path)
}

fn build_boot_disk(elf: &Path, dist_root: &Path, arch: &str, test: bool) -> Result<()> {
    let rootfs = ensure_rootfs_image()?;
    let rootfs_size = fs::metadata(&rootfs)?.len();
    let layout = default_disk_layout(rootfs_size);

    let image_name = if test {
        format!("ion_os_{arch}_test.img")
    } else {
        format!("ion_os_{arch}.img")
    };
    let disk_image = dist_root.join(image_name);

    write_gpt_disk_image(&disk_image, layout)?;
    copy_rootfs_into_disk(&disk_image, layout.root, &rootfs)?;

    let grub_cfg = dist_root.join("grub-disk.cfg");
    fs::write(
        &grub_cfg,
        "set timeout=0\nset default=0\nset root=(hd0,gpt2)\nmenuentry \"Ion OS\" {\n    multiboot2 /kernel.elf\n    boot\n}\n",
    )?;

    let efi_boot = dist_root.join("BOOTX64.EFI");
    let cfg_binding = grub_cfg.display().to_string();
    let efi_binding = efi_boot.display().to_string();
    docker(&[
        "grub-mkstandalone",
        "-d",
        "/usr/lib/grub/x86_64-efi",
        "-O",
        "x86_64-efi",
        "-o",
        &efi_binding,
        &format!("boot/grub/grub.cfg={}", cfg_binding),
    ])?;

    populate_esp(&disk_image, layout.esp, elf, &grub_cfg, &efi_boot)?;
    install_bios_grub(&disk_image, layout.esp, &grub_cfg, dist_root)?;
    Ok(())
}

fn write_gpt_disk_image(path: &Path, layout: DiskLayout) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.set_len(layout.total_bytes)?;

    let total_lbas = layout.total_bytes / BLOCK_SIZE as u64;
    let partition_entries_lba = 2u64;
    let partition_entries_lbas = bytes_to_lba((GPT_ENTRY_COUNT * GPT_ENTRY_SIZE) as u64);
    let backup_entries_lba = total_lbas - 1 - partition_entries_lbas;
    let backup_header_lba = total_lbas - 1;

    let entries = build_partition_entries(layout);
    let entries_crc = crc32fast::hash(&entries);

    write_protective_mbr(&mut file, total_lbas)?;

    file.seek(SeekFrom::Start(partition_entries_lba * BLOCK_SIZE as u64))?;
    file.write_all(&entries)?;

    file.seek(SeekFrom::Start(backup_entries_lba * BLOCK_SIZE as u64))?;
    file.write_all(&entries)?;

    let primary = build_gpt_header(
        1,
        backup_header_lba,
        layout.bios_grub.first_lba,
        layout.root.last_lba,
        partition_entries_lba,
        entries_crc,
    );

    let backup = build_gpt_header(
        backup_header_lba,
        1,
        layout.bios_grub.first_lba,
        layout.root.last_lba,
        backup_entries_lba,
        entries_crc,
    );

    file.seek(SeekFrom::Start(BLOCK_SIZE as u64))?;
    file.write_all(&primary)?;

    file.seek(SeekFrom::Start(backup_header_lba * BLOCK_SIZE as u64))?;
    file.write_all(&backup)?;
    file.sync_all()?;
    Ok(())
}

fn build_partition_entries(layout: DiskLayout) -> Vec<u8> {
    let mut entries = vec![0u8; GPT_ENTRY_COUNT * GPT_ENTRY_SIZE];
    let partitions = [
        (layout.bios_grub, "BIOSBOOT"),
        (layout.esp, "EFI"),
        (layout.root, "IONROOT"),
    ];

    for (slot, (part, name)) in partitions.into_iter().enumerate() {
        let entry = &mut entries[slot * GPT_ENTRY_SIZE..(slot + 1) * GPT_ENTRY_SIZE];
        entry[0..16].copy_from_slice(&part.type_guid);
        entry[16..32].copy_from_slice(&part.unique_guid);
        entry[32..40].copy_from_slice(&part.first_lba.to_le_bytes());
        entry[40..48].copy_from_slice(&part.last_lba.to_le_bytes());
        entry[56..128].copy_from_slice(&encode_gpt_name(name));
    }

    entries
}

fn build_gpt_header(
    current_lba: u64,
    backup_lba: u64,
    first_usable_lba: u64,
    last_usable_lba: u64,
    entries_lba: u64,
    entries_crc: u32,
) -> [u8; BLOCK_SIZE] {
    let mut header = [0u8; BLOCK_SIZE];
    header[0..8].copy_from_slice(GPT_HEADER_SIGNATURE);
    header[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    header[12..16].copy_from_slice(&92u32.to_le_bytes());
    header[24..32].copy_from_slice(&current_lba.to_le_bytes());
    header[32..40].copy_from_slice(&backup_lba.to_le_bytes());
    header[40..48].copy_from_slice(&first_usable_lba.to_le_bytes());
    header[48..56].copy_from_slice(&last_usable_lba.to_le_bytes());
    header[56..72].copy_from_slice(&DISK_GUID);
    header[72..80].copy_from_slice(&entries_lba.to_le_bytes());
    header[80..84].copy_from_slice(&(GPT_ENTRY_COUNT as u32).to_le_bytes());
    header[84..88].copy_from_slice(&(GPT_ENTRY_SIZE as u32).to_le_bytes());
    header[88..92].copy_from_slice(&entries_crc.to_le_bytes());

    let crc = crc32fast::hash(&header[..92]);
    header[16..20].copy_from_slice(&crc.to_le_bytes());
    header
}

fn write_protective_mbr(file: &mut fs::File, total_lbas: u64) -> Result<()> {
    let mut mbr = [0u8; BLOCK_SIZE];
    let entry = &mut mbr[446..462];
    entry[4] = 0xEE;
    entry[8..12].copy_from_slice(&1u32.to_le_bytes());
    entry[12..16].copy_from_slice(&(total_lbas.saturating_sub(1).min(u32::MAX as u64) as u32).to_le_bytes());
    mbr[510] = 0x55;
    mbr[511] = 0xAA;

    file.seek(SeekFrom::Start(0))?;
    file.write_all(&mbr)?;
    Ok(())
}

fn copy_rootfs_into_disk(disk_image: &Path, root: GptPartition, rootfs: &Path) -> Result<()> {
    let mut src = fs::File::open(rootfs)?;
    let mut dst = fs::OpenOptions::new().read(true).write(true).open(disk_image)?;

    dst.seek(SeekFrom::Start(root.byte_offset()))?;
    std::io::copy(&mut src, &mut dst)?;
    dst.sync_all()?;
    Ok(())
}

fn populate_esp(disk_image: &Path, esp: GptPartition, elf: &Path, grub_cfg: &Path, efi_boot: &Path) -> Result<()> {
    let disk_binding = disk_image.display().to_string();
    let esp_offset_sectors = esp.first_lba.to_string();
    docker(&[
        "mkfs.fat",
        "-F",
        "32",
        "--offset",
        &esp_offset_sectors,
        &disk_binding,
    ])?;

    let image_binding = format!("{}@@{}", disk_image.display(), esp.byte_offset());
    docker(&["mmd", "-i", &image_binding, "::/EFI"])?;
    docker(&["mmd", "-i", &image_binding, "::/EFI/BOOT"])?;
    docker(&["mmd", "-i", &image_binding, "::/boot"])?;
    docker(&["mmd", "-i", &image_binding, "::/boot/grub"])?;

    let elf_binding = elf.display().to_string();
    let cfg_binding = grub_cfg.display().to_string();
    let efi_binding = efi_boot.display().to_string();

    docker(&["mcopy", "-i", &image_binding, &elf_binding, "::/kernel.elf"])?;
    docker(&["mcopy", "-i", &image_binding, &cfg_binding, "::/boot/grub/grub.cfg"])?;
    docker(&["mcopy", "-i", &image_binding, &efi_binding, "::/EFI/BOOT/BOOTX64.EFI"])?;
    Ok(())
}

fn install_bios_grub(disk_image: &Path, esp: GptPartition, grub_cfg: &Path, dist_root: &Path) -> Result<()> {
    let _ = grub_cfg;
    let _ = dist_root;
    let disk_binding = format!("/root/env/{}", disk_image.display());
    let esp_mount = "/tmp/ion-esp";
    let boot_dir_binding = format!("{esp_mount}/boot");
    let esp_offset = esp.byte_offset();

    let script = format!(
        "set -e\n\
         loop=$(losetup -f --show '{disk}')\n\
         trap 'umount {esp} >/dev/null 2>&1 || true; losetup -d \"$loop\" >/dev/null 2>&1 || true' EXIT\n\
         mkdir -p {esp}\n\
         mount -t vfat -o offset={offset} \"$loop\" {esp}\n\
         grub-install --target=i386-pc --boot-directory='{boot}' --modules='biosdisk part_gpt fat multiboot2 normal configfile search' --install-modules='biosdisk part_gpt fat multiboot2 normal configfile search' --no-floppy --force --skip-fs-probe \"$loop\"\n\
         umount {esp}\n",
        disk = disk_binding,
        esp = esp_mount,
        boot = boot_dir_binding,
        offset = esp_offset,
    );

    docker_privileged(&["sh", "-lc", &script])?;

    Ok(())
}

//
// ---------------- run ----------------
//

fn run_vm(vm: Vm, arch: String, test: bool, firmware: Firmware) -> Result<()> {
    let mut disk = format!("dist/{}/", arch);

    if test {
        disk.push_str(&format!("test/ion_os_{arch}_test.img"));
    } else {
        disk.push_str(&format!("ion_os_{arch}.img"));
    }

    match vm {
        Vm::Qemu => run_qemu(&disk, firmware),
        Vm::Bochs => bail!("bochs backend not implemented yet"),
    }
}

fn run_qemu(disk_image: &str, firmware: Firmware) -> Result<()> {
    let mut binding = Command::new("qemu-system-x86_64");
    binding.args([
        "-debugcon",
        "stdio",
        "-device",
        "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-drive",
        &format!("file={disk_image},if=none,id=disk0,format=raw"),
        "-device", 
        "virtio-blk-pci,drive=disk0,disable-legacy=on",
        "-machine",
        "q35",
        "-d",
        "cpu_reset",
        "-cpu", 
        "qemu64,+sse2,+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+avx,+avx2,+xsave,+xsaveopt",
        "-smp",
        "2"
    ]);

    if matches!(firmware, Firmware::Uefi) {
        let ovmf = [
            "/usr/share/OVMF/OVMF_CODE.fd",
            "/usr/share/edk2/x64/OVMF_CODE.fd",
            "/usr/share/edk2-ovmf/x64/OVMF_CODE.fd",
        ]
        .into_iter()
        .find(|path| Path::new(path).exists())
        .ok_or_else(|| anyhow!("UEFI requested but no OVMF firmware image was found in a standard location"))?;

        binding.args(["-bios", ovmf]);
    }

    let status = binding.status()?.code();
    if status != Some(33) {
        // qemu exit code for `OK`
        bail!("qemu failed: {:?}", binding);
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

        

        #[allow(clippy::if_same_then_else)]
        match buf.as_str().trim() {
            "1" => {
                if minimal {
                    ["qemu-system-x86_64"]
                } else {
                    ["qemu-system-x86_64"] // TODO: add more archs.
                }
            }
            v => bail!("unknown option: {v}"),
        }
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

// mkfs

/// TODO: make dynamic
const BLOCK_SIZE: usize = 512;

const SUPER_MAGIC: u32 = 0xab87beef;
const ALLOC_MAGIC: u32 = 0xab87befd;

const VERSION: [u8; 4] = [0, 1, 0, 0];

const GPT_HEADER_SIGNATURE: &[u8; 8] = b"EFI PART";
const GPT_ENTRY_COUNT: usize = 128;
const GPT_ENTRY_SIZE: usize = 128;

const BIOS_GRUB_PARTITION_SIZE_BYTES: u64 = 1024 * 1024;
const ESP_PARTITION_SIZE_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_ROOTFS_SIZE_BYTES: u64 = 64 * 1024 * 1024;
const IMAGE_ALIGNMENT_BYTES: u64 = 1024 * 1024;

const BIOS_BOOT_TYPE_GUID: [u8; 16] = [
    0x48, 0x61, 0x68, 0x21, 0x49, 0x64, 0x6F, 0x6E,
    0x74, 0x4E, 0x65, 0x65, 0x64, 0x45, 0x46, 0x49,
];

const EFI_SYSTEM_TYPE_GUID: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11,
    0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
];

const LINUX_FILESYSTEM_TYPE_GUID: [u8; 16] = [
    0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47,
    0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D, 0xE4,
];

const BIOS_PARTITION_GUID: [u8; 16] = [0x11; 16];
const EFI_PARTITION_GUID: [u8; 16] = [0x22; 16];
const ROOT_PARTITION_GUID: [u8; 16] = [0x33; 16];
const DISK_GUID: [u8; 16] = [0x44; 16];

#[derive(Clone, Copy, Debug)]
struct GptPartition {
    index: u32,
    type_guid: [u8; 16],
    unique_guid: [u8; 16],
    first_lba: u64,
    last_lba: u64,
}

impl GptPartition {
    fn byte_offset(self) -> u64 {
        self.first_lba * BLOCK_SIZE as u64
    }

    // fn byte_len(self) -> u64 {
    //     (self.last_lba - self.first_lba + 1) * BLOCK_SIZE as u64
    // }
}

#[derive(Clone, Copy, Debug)]
struct DiskLayout {
    bios_grub: GptPartition,
    esp: GptPartition,
    root: GptPartition,
    total_bytes: u64,
}

struct OffsetFile {
    inner: fs::File,
    base_offset: u64,
}

impl OffsetFile {
    fn open(path: &Path, write: bool, partition: Option<u32>) -> Result<Self> {
        let inner = fs::OpenOptions::new()
            .read(true)
            .write(write)
            .open(path)?;

        let base_offset = resolve_ionfs_base_offset(path, partition)?;
        Ok(Self { inner, base_offset })
    }

    // fn create(path: &Path, truncate: bool) -> Result<Self> {
    //     let inner = fs::OpenOptions::new()
    //         .read(true)
    //         .write(true)
    //         .create(true)
    //         .truncate(truncate)
    //         .open(path)?;

    //     Ok(Self {
    //         inner,
    //         base_offset: 0,
    //     })
    // }

    fn sync_all(&self) -> Result<()> {
        self.inner.sync_all()?;
        Ok(())
    }
}

impl Read for OffsetFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Write for OffsetFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl Seek for OffsetFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match pos {
            SeekFrom::Start(offset) => self.inner.seek(SeekFrom::Start(self.base_offset + offset)),
            other => self.inner.seek(other),
        }
    }
}

fn align_up(value: u64, align: u64) -> u64 {
    if value.is_multiple_of(align) {
        value
    } else {
        value + (align - (value % align))
    }
}

fn bytes_to_lba(bytes: u64) -> u64 {
    bytes.div_ceil(BLOCK_SIZE as u64)
}

fn encode_gpt_name(name: &str) -> [u8; 72] {
    let mut buf = [0u8; 72];

    for (idx, unit) in name.encode_utf16().take(36).enumerate() {
        let bytes = unit.to_le_bytes();
        let off = idx * 2;
        buf[off] = bytes[0];
        buf[off + 1] = bytes[1];
    }

    buf
}

fn default_disk_layout(rootfs_bytes: u64) -> DiskLayout {
    let partition_entries_lba = 2u64;
    let partition_entries_bytes = (GPT_ENTRY_COUNT * GPT_ENTRY_SIZE) as u64;
    let partition_entries_lbas = bytes_to_lba(partition_entries_bytes);

    let first_usable_lba = align_up(
        (partition_entries_lba + partition_entries_lbas) * BLOCK_SIZE as u64,
        IMAGE_ALIGNMENT_BYTES,
    ) / BLOCK_SIZE as u64;

    let bios_blocks = bytes_to_lba(BIOS_GRUB_PARTITION_SIZE_BYTES);
    let esp_blocks = bytes_to_lba(ESP_PARTITION_SIZE_BYTES);
    let root_blocks = bytes_to_lba(rootfs_bytes.max(DEFAULT_ROOTFS_SIZE_BYTES));

    let bios_first = first_usable_lba;
    let bios_last = bios_first + bios_blocks - 1;

    let esp_first = align_up((bios_last + 1) * BLOCK_SIZE as u64, IMAGE_ALIGNMENT_BYTES) / BLOCK_SIZE as u64;
    let esp_last = esp_first + esp_blocks - 1;

    let root_first = align_up((esp_last + 1) * BLOCK_SIZE as u64, IMAGE_ALIGNMENT_BYTES) / BLOCK_SIZE as u64;
    let root_last = root_first + root_blocks - 1;

    let backup_entries_lba = root_last + 1;
    let backup_header_lba = backup_entries_lba + partition_entries_lbas;
    let total_bytes = (backup_header_lba + 1) * BLOCK_SIZE as u64;

    DiskLayout {
        bios_grub: GptPartition {
            index: 1,
            type_guid: BIOS_BOOT_TYPE_GUID,
            unique_guid: BIOS_PARTITION_GUID,
            first_lba: bios_first,
            last_lba: bios_last,
        },
        esp: GptPartition {
            index: 2,
            type_guid: EFI_SYSTEM_TYPE_GUID,
            unique_guid: EFI_PARTITION_GUID,
            first_lba: esp_first,
            last_lba: esp_last,
        },
        root: GptPartition {
            index: 3,
            type_guid: LINUX_FILESYSTEM_TYPE_GUID,
            unique_guid: ROOT_PARTITION_GUID,
            first_lba: root_first,
            last_lba: root_last,
        },
        total_bytes,
    }
}

fn read_gpt_partitions(path: &Path) -> Result<Vec<GptPartition>> {
    let mut file = fs::File::open(path)?;
    let mut header = [0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(BLOCK_SIZE as u64))?;
    file.read_exact(&mut header)?;

    if &header[0..8] != GPT_HEADER_SIGNATURE {
        return Ok(Vec::new());
    }

    let entry_lba = u64::from_le_bytes(header[72..80].try_into()?);
    let entry_count = u32::from_le_bytes(header[80..84].try_into()?) as usize;
    let entry_size = u32::from_le_bytes(header[84..88].try_into()?) as usize;

    if entry_size < GPT_ENTRY_SIZE {
        bail!("unsupported GPT entry size: {}", entry_size);
    }

    let mut entries = vec![0u8; entry_count * entry_size];
    file.seek(SeekFrom::Start(entry_lba * BLOCK_SIZE as u64))?;
    file.read_exact(&mut entries)?;

    let mut partitions = Vec::new();
    for (idx, entry) in entries.chunks(entry_size).enumerate() {
        if entry[..16].iter().all(|b| *b == 0) {
            continue;
        }

        partitions.push(GptPartition {
            index: (idx + 1) as u32,
            type_guid: entry[0..16].try_into()?,
            unique_guid: entry[16..32].try_into()?,
            first_lba: u64::from_le_bytes(entry[32..40].try_into()?),
            last_lba: u64::from_le_bytes(entry[40..48].try_into()?),
        });
    }

    Ok(partitions)
}

fn resolve_ionfs_base_offset(path: &Path, partition: Option<u32>) -> Result<u64> {
    let mut file = fs::File::open(path)?;
    let mut superblock = [0u8; 4];
    file.read_exact(&mut superblock)?;
    if u32::from_le_bytes(superblock) == SUPER_MAGIC {
        return Ok(0);
    }

    let partitions = read_gpt_partitions(path)?;
    if partitions.is_empty() {
        return Ok(0);
    }

    if let Some(index) = partition {
        let part = partitions
            .into_iter()
            .find(|part| part.index == index)
            .ok_or_else(|| anyhow!("GPT partition {} was not found", index))?;
        return Ok(part.byte_offset());
    }

    for part in partitions {
        file.seek(SeekFrom::Start(part.byte_offset()))?;
        file.read_exact(&mut superblock)?;
        if u32::from_le_bytes(superblock) == SUPER_MAGIC {
            return Ok(part.byte_offset());
        }
    }

    bail!("failed to find an IonFS partition in {}", path.display())
}


fn write_block(
    file: &mut OffsetFile,
    block: u64,
    buf: &[u8],
) -> anyhow::Result<()> {
    assert_eq!(buf.len(), BLOCK_SIZE);
    file.seek(SeekFrom::Start(block * BLOCK_SIZE as u64))?;
    file.write_all(buf)?;
    Ok(())
}

fn write_superblock(file: &mut OffsetFile) -> Result<()> {
    let mut buf = vec![0u8; BLOCK_SIZE];

    buf[0..4].copy_from_slice(&SUPER_MAGIC.to_le_bytes());
    buf[4..8].copy_from_slice(&VERSION);
    buf[8..16].copy_from_slice(&(BLOCK_SIZE as u64).to_le_bytes());

    write_block(file, 0, &buf)
}

fn write_alloc_block(file: &mut OffsetFile) -> Result<()> {
    let mut buf = vec![0u8; BLOCK_SIZE];

    buf[0..4].copy_from_slice(&ALLOC_MAGIC.to_le_bytes());
    buf[4..8].copy_from_slice(&VERSION);
    buf[8..16].copy_from_slice(&4u64.to_le_bytes()); // next free block (after root data at block 3)
    buf[16..24].copy_from_slice(&1u64.to_le_bytes()); // inode block count
    buf[24..32].copy_from_slice(&2u64.to_le_bytes()); // inode block #1

    let mut inode_table_buf = vec![0u8; BLOCK_SIZE];

    // free hint
    inode_table_buf[0..2].copy_from_slice(&1u16.to_le_bytes());

    // root inode at offset 4
    // Root inode points to block 3 for its data
    let root_inode_bytes = [
        1, // used
        1, // Directory
        /* data_size */   0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 512 bytes
        /* data_offset */ 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // block 3 = byte offset 1536
        /* size */        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 0 bytes used initially
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // index 0
    ];

    // warn: change when inode size changes
    inode_table_buf[4 .. 4 + 34]
        .copy_from_slice(&root_inode_bytes);

    // Write empty root directory data block
    let root_data = vec![0u8; BLOCK_SIZE];
    write_block(file, 3, &root_data)?;

    write_block(file, 2, &inode_table_buf)?;

    write_block(file, 1, &buf)
}

fn fs_make(path: &std::path::Path, partition: Option<u32>) -> Result<()> {
    let mut file = OffsetFile::open(path, true, partition)?;

    write_superblock(&mut file)?;
    write_alloc_block(&mut file)?;

    file.sync_all()?;
    println!("[fs] Filesystem created at {}", path.display());
    Ok(())
}

fn fs_read(path: &std::path::Path, partition: Option<u32>, read_inodes: &[u64]) -> Result<()> {
    let mut file = OffsetFile::open(path, false, partition)?;

    // Read and verify superblock
    let mut superblock = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut superblock)?;

    let magic = u32::from_le_bytes([
        superblock[0],
        superblock[1],
        superblock[2],
        superblock[3],
    ]);

    if magic != SUPER_MAGIC {
        anyhow::bail!("Invalid superblock magic: {:#x}", magic);
    }

    let version = [
        superblock[4],
        superblock[5],
        superblock[6],
        superblock[7],
    ];

    println!("[fs] Superblock:");
    println!("  Magic: {:#x}", magic);
    println!("  Version: {}.{}.{}", version[0], version[1], u16::from_le_bytes([version[2], version[3]]));

    // Read allocation block
    let mut alloc_block = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(BLOCK_SIZE as u64))?;
    file.read_exact(&mut alloc_block)?;

    let alloc_magic = u32::from_le_bytes([
        alloc_block[0],
        alloc_block[1],
        alloc_block[2],
        alloc_block[3],
    ]);

    if alloc_magic != ALLOC_MAGIC {
        anyhow::bail!("Invalid allocation block magic: {:#x}", alloc_magic);
    }

    let next_free = u64::from_le_bytes(alloc_block[8..16].try_into()?);
    let inode_block_count = u64::from_le_bytes(alloc_block[16..24].try_into()?);

    println!("[fs] Allocation Block:");
    println!("  Magic: {:#x}", alloc_magic);
    println!("  Next Free Block: {}", next_free);
    println!("  Inode Block Count: {}", inode_block_count);

    // Read inode blocks and display structure
    let inodes_per_block = (BLOCK_SIZE - 4) / 34; // (512 - 4 header) / 34 bytes per inode
    
    println!("\n[fs] Filesystem Structure:");
    let mut all_inodes: Vec<(u64, Inode)> = Vec::new();

    for block_idx in 0..inode_block_count {
        let block_num = u64::from_le_bytes(
            alloc_block[24 + (block_idx as usize * 8)..24 + (block_idx as usize * 8) + 8].try_into()?
        );
        
        let mut inode_block = vec![0u8; BLOCK_SIZE];
        file.seek(SeekFrom::Start(block_num * BLOCK_SIZE as u64))?;
        file.read_exact(&mut inode_block)?;

        let free_hint = u16::from_le_bytes([inode_block[0], inode_block[1]]);
        println!("  Inode Block {} (disk block {}):", block_idx, block_num);
        println!("    Free Hint: {}", if free_hint == 0xFFFF { "FULL".to_string() } else { free_hint.to_string() });

        // Display used inodes
        let mut inode_count = 0;
        for inode_slot in 0..inodes_per_block {
            let offset = 4 + (inode_slot * 34);
            if offset + 34 > BLOCK_SIZE {
                break;
            }
            
            let used = inode_block[offset] != 0;
            if used {
                let inode = parse_inode(&inode_block[offset..offset+34])?;
                let global_inode_idx = block_idx * inodes_per_block as u64 + inode_slot as u64;
                
                let kind_str = match inode.kind {
                    0 => "File",
                    1 => "Dir",
                    _ => "Unknown",
                };
                
                println!("    Inode {}: Type={}, Size={}/{} bytes, Offset={}", 
                    global_inode_idx, kind_str, inode.size, inode.data_size, inode.data_offset);
                
                all_inodes.push((global_inode_idx, inode));
                inode_count += 1;
            }
        }
        
        if inode_count == 0 {
            println!("    (no inodes)");
        }
    }

    // Now display directory tree like `tree` command
    println!("\n[fs] Directory Tree:");
    println!("@/");
    print_dir_tree(&mut file, 0, "", &all_inodes)?;

    // Display file contents only if explicitly requested
    if !read_inodes.is_empty() {
        println!("\n[fs] File Contents:");
        for inode_idx in read_inodes {
            if let Some((_, inode)) = all_inodes.iter().find(|(idx, _)| *idx == *inode_idx) {
                if inode.kind == 0 { // File
                    println!("  Inode {} ({} bytes):", inode_idx, inode.size);
                    match read_file_contents(&mut file, inode) {
                        std::result::Result::Ok(contents) => {
                            if contents.is_empty() {
                                println!("    (empty)");
                            } else {
                                let preview = if contents.len() > 256 {
                                    format!("{}\n    ... ({} more bytes)", 
                                        String::from_utf8_lossy(&contents[..256]),
                                        contents.len() - 256)
                                } else {
                                    String::from_utf8_lossy(&contents).to_string()
                                };
                                for line in preview.lines() {
                                    println!("    {}", line);
                                }
                            }
                        }
                        Err(e) => println!("    Error reading file: {}", e),
                    }
                } else {
                    println!("  Inode {} is not a file (type: {})", inode_idx, if inode.kind == 1 { "Directory" } else { "Unknown" });
                }
            } else {
                println!("  Inode {} not found", inode_idx);
            }
        }
    }

    println!("\n[fs] Filesystem structure verified successfully");
    Ok(())
}

fn print_dir_tree(
    file: &mut OffsetFile,
    inode_idx: u64,
    prefix: &str,
    all_inodes: &[(u64, Inode)],
) -> Result<()> {
    if let Some((_, inode)) = all_inodes.iter().find(|(idx, _)| *idx == inode_idx) {
        if inode.kind != 1 {
            return Ok(());
        }

        match read_directory(file, inode) {
            std::result::Result::Ok(entries) => {
                let entry_count = entries.len();
                for (i, (name, target_inode)) in entries.iter().enumerate() {
                    let name_str = String::from_utf8_lossy(name);
                    let is_last = i == entry_count - 1;
                    let connector = if is_last { "└── " } else { "├── " };
                    
                    if let Some((_, target)) = all_inodes.iter().find(|(idx, _)| *idx == *target_inode) {
                        // Show directory with / suffix, file as-is
                        let suffix = if target.kind == 1 { "/" } else { "" };
                        println!("{}{}{}{}", prefix, connector, name_str, suffix);
                        
                        // Recursively print subdirectories
                        if target.kind == 1 {
                            let next_prefix = format!("{}{}    ", prefix, if is_last { " " } else { "│" });
                            print_dir_tree(file, *target_inode, &next_prefix, all_inodes)?;
                        }
                    } else {
                        println!("{}{}{} (inode {} - broken link)", prefix, connector, name_str, target_inode);
                    }
                }
            }
            Err(e) => println!("{}Error reading directory: {}", prefix, e),
        }
    }

    Ok(())
}

#[allow(dead_code)]
fn print_all_dirs(all_inodes: &[(u64, Inode)]) -> Result<()> {
    // Show all directories and their inode numbers
    let dirs: Vec<_> = all_inodes.iter()
        .filter(|(_, inode)| inode.kind == 1)
        .collect();
    
    if dirs.is_empty() {
        println!("  (no directories)");
        return Ok(());
    }

    for (inode_idx, _) in dirs {
        println!("  Inode {}: [Directory]", inode_idx);
    }
    
    Ok(())
}

fn read_file_contents(file: &mut OffsetFile, inode: &Inode) -> Result<Vec<u8>> {
    let mut contents = vec![0u8; inode.size as usize];
    file.seek(SeekFrom::Start(inode.data_offset))?;
    file.read_exact(&mut contents)?;
    Ok(contents)
}

#[derive(Clone)]
struct Inode {
    used: bool,
    kind: u8,
    data_size: u64,
    data_offset: u64,
    size: u64,
    index: u64
}

fn parse_inode(buf: &[u8]) -> Result<Inode> {
    if buf.len() < 34 {
        anyhow::bail!("Buffer too small for inode");
    }
    Ok(Inode {
        used: buf[0] != 0,
        kind: buf[1],
        data_size: u64::from_le_bytes(buf[2..10].try_into()?),
        data_offset: u64::from_le_bytes(buf[10..18].try_into()?),
        size: u64::from_le_bytes(buf[18..26].try_into()?),
        index: u64::from_le_bytes(buf[26..34].try_into()?),
    })
}

fn read_directory(file: &mut OffsetFile, inode: &Inode) -> Result<Vec<(Vec<u8>, u64)>> {
    if inode.size == 0 {
        return Ok(Vec::new());
    }

    let mut buf = vec![0u8; inode.size as usize];
    file.seek(SeekFrom::Start(inode.data_offset))?;
    file.read_exact(&mut buf)?;

    let mut entries = Vec::new();
    let mut offset = 0;

    while offset < buf.len() {
        if offset + 10 > buf.len() {
            break;
        }

        // Read name length (u16)
        let name_len = u16::from_le_bytes([buf[offset], buf[offset + 1]]) as usize;
        offset += 2;

        if offset + name_len + 8 > buf.len() {
            break;
        }

        // Read name
        let name = buf[offset..offset + name_len].to_vec();
        offset += name_len;

        // Read inode index (u64)
        let inode_idx = u64::from_le_bytes(buf[offset..offset + 8].try_into()?);
        offset += 8;

        entries.push((name, inode_idx));
    }

    Ok(entries)
}

fn fs_ls(path: &std::path::Path, partition: Option<u32>, dir: &str, long: bool, _all: bool) -> Result<()> {
    let mut file = OffsetFile::open(path, false, partition)?;

    // Read allocation block to get inode block location
    let mut alloc_block = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(BLOCK_SIZE as u64))?;
    file.read_exact(&mut alloc_block)?;

    let inode_block_count = u64::from_le_bytes(alloc_block[16..24].try_into()?);
    if inode_block_count == 0 {
        anyhow::bail!("No inode blocks found");
    }

    let inode_block_num = u64::from_le_bytes(alloc_block[24..32].try_into()?);

    // Get all inodes for path navigation
    let all_inodes = read_all_inodes(&mut file, inode_block_num)?;

    // Parse the directory path and find the target inode
    let target_inode_idx = if dir == "/" {
        0u64
    } else {
        navigate_to_dir(&mut file, &all_inodes, dir)?
    };

    // Read the target directory inode
    let target_offset = inode_block_num * BLOCK_SIZE as u64 + 4 + (target_inode_idx as usize * 34) as u64;
    let mut target_inode_buf = vec![0u8; 34];
    file.seek(SeekFrom::Start(target_offset))?;
    file.read_exact(&mut target_inode_buf)?;
    let target_inode = parse_inode(&target_inode_buf)?;

    if target_inode.kind != 1 {
        anyhow::bail!("{} is not a directory", dir);
    }

    // Read target directory data
    let mut target_data = vec![0u8; target_inode.size as usize];
    file.seek(SeekFrom::Start(target_inode.data_offset))?;
    file.read_exact(&mut target_data)?;

    // Parse and display directory entries
    let mut pos = 0;
    if long {
        println!("Inode       Size  Name");
        println!("---------  -----  ----");
    }

    while pos < target_data.len() {
        if pos + 2 > target_data.len() {
            break;
        }

        let name_len = u16::from_le_bytes([target_data[pos], target_data[pos+1]]) as usize;
        pos += 2;

        if pos + name_len + 8 > target_data.len() {
            break;
        }

        let entry_name = String::from_utf8_lossy(&target_data[pos..pos+name_len]);
        let entry_inode_idx = u64::from_le_bytes(
            target_data[pos+name_len..pos+name_len+8].try_into()?
        );

        // Read the inode to get size and type
        let entry_inode_offset = inode_block_num * BLOCK_SIZE as u64 + 4 + (entry_inode_idx as usize * 34) as u64;
        let mut entry_inode_buf = vec![0u8; 34];
        file.seek(SeekFrom::Start(entry_inode_offset))?;
        file.read_exact(&mut entry_inode_buf)?;
        let entry_inode = parse_inode(&entry_inode_buf)?;

        // Format output based on flags
        if long {
            let kind_char = if entry_inode.kind == 1 { "/" } else { "" };
            println!("{:9}  {:5}  {}{}", entry_inode_idx, entry_inode.size, entry_name, kind_char);
        } else {
            let kind_marker = if entry_inode.kind == 1 { "/" } else { "" };
            print!("{}{} ", entry_name, kind_marker);
        }

        pos += name_len + 8;
    }

    if !long {
        println!(); // newline at end for normal output
    }

    Ok(())
}

fn fs_cat(path: &std::path::Path, partition: Option<u32>, file_path: &str) -> Result<()> {
    let mut file = OffsetFile::open(path, false, partition)?;

    // Read allocation block
    let mut alloc_block = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(BLOCK_SIZE as u64))?;
    file.read_exact(&mut alloc_block)?;

    let inode_block_count = u64::from_le_bytes(alloc_block[16..24].try_into()?);
    if inode_block_count == 0 {
        anyhow::bail!("No inode blocks found");
    }

    let inode_block_num = u64::from_le_bytes(alloc_block[24..32].try_into()?);

    // Get all inodes for path navigation
    let all_inodes = read_all_inodes(&mut file, inode_block_num)?;

    // Parse path to get target file inode
    let (parent_inode_idx, filename) = parse_path(&mut file, &all_inodes, file_path)?;

    // Read parent directory to find the file
    let parent_offset = inode_block_num * BLOCK_SIZE as u64 + 4 + (parent_inode_idx as usize * 34) as u64;
    let mut parent_inode_buf = vec![0u8; 34];
    file.seek(SeekFrom::Start(parent_offset))?;
    file.read_exact(&mut parent_inode_buf)?;
    let parent_inode = parse_inode(&parent_inode_buf)?;

    // Read parent directory data
    let mut parent_data = vec![0u8; parent_inode.size as usize];
    file.seek(SeekFrom::Start(parent_inode.data_offset))?;
    file.read_exact(&mut parent_data)?;

    // Find the file in parent directory
    let mut pos = 0;
    let mut file_inode_idx = None;

    while pos < parent_data.len() {
        if pos + 2 > parent_data.len() {
            break;
        }

        let name_len = u16::from_le_bytes([parent_data[pos], parent_data[pos+1]]) as usize;
        pos += 2;

        if pos + name_len + 8 > parent_data.len() {
            break;
        }

        let entry_name = String::from_utf8_lossy(&parent_data[pos..pos+name_len]);
        let entry_inode_idx = u64::from_le_bytes(
            parent_data[pos+name_len..pos+name_len+8].try_into()?
        );

        if entry_name.as_ref() == filename {
            file_inode_idx = Some(entry_inode_idx);
            break;
        }

        pos += name_len + 8;
    }

    let target_inode_idx = file_inode_idx.ok_or_else(|| anyhow::anyhow!("File not found: @/{}", file_path))?;

    // Read the target file inode
    let target_offset = inode_block_num * BLOCK_SIZE as u64 + 4 + (target_inode_idx as usize * 34) as u64;
    let mut target_inode_buf = vec![0u8; 34];
    file.seek(SeekFrom::Start(target_offset))?;
    file.read_exact(&mut target_inode_buf)?;
    let target_inode = parse_inode(&target_inode_buf)?;

    if target_inode.kind == 1 {
        anyhow::bail!("@/{} is a directory, not a file", file_path);
    }

    // Read and display file contents
    let mut file_data = vec![0u8; target_inode.size as usize];
    file.seek(SeekFrom::Start(target_inode.data_offset))?;
    file.read_exact(&mut file_data)?;

    // Print as text, with error handling for non-UTF8
    match String::from_utf8(file_data.clone()) {
        std::result::Result::Ok(content) => print!("{}", content),
        std::result::Result::Err(_) => {
            eprintln!("Warning: File contains non-UTF8 data, displaying as hex");
            for byte in file_data.iter() {
                print!("{:02x} ", byte);
            }
            println!();
        }
    }

    std::result::Result::Ok(())
}

fn fs_port(path: &std::path::Path, partition: Option<u32>, dest: &str, files: &[PathBuf], recursive: bool) -> Result<()> {
    if files.is_empty() {
        println!("[fs] No files specified to port");
        return Ok(());
    }

    let mut file = OffsetFile::open(path, true, partition)?;

    // Read allocation block
    let mut alloc_block = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(BLOCK_SIZE as u64))?;
    file.read_exact(&mut alloc_block)?;

    let mut next_free = u64::from_le_bytes(alloc_block[8..16].try_into()?);
    let inode_block_count = u64::from_le_bytes(alloc_block[16..24].try_into()?);
    
    if inode_block_count == 0 {
        anyhow::bail!("No inode blocks found. Initialize filesystem first.");
    }

    let inode_block_num = u64::from_le_bytes(alloc_block[24..32].try_into()?);
    let inodes_per_block = (BLOCK_SIZE - 4) / 34;

    // Normalize destination
    let dest = dest.trim_start_matches('/').trim_end_matches('/');
    let dest = if dest.is_empty() { "/" } else { dest };

    if !recursive && dest != "/" {
        anyhow::bail!("Non-root destination requires --recursive flag\n\nUsage:\n  cargo xtask fs port --recursive --path dist/disk.img [FILES...]\n\nThe --recursive flag is required when:\n  • Porting to a non-root destination (e.g., @/docs)\n  • Porting directories (automatic structure preservation)\n  • Porting multiple files from different locations");
    }

    println!("[fs] Porting {} file(s) to {} {}", 
        files.len(), dest, 
        if recursive { "(recursive)" } else { "" });

    // Collect all files to port (recursively if needed)
    let mut items_to_process: Vec<(PathBuf, String)> = Vec::new();

    for src_path in files {
        if !src_path.exists() {
            anyhow::bail!("Path not found: {}", src_path.display());
        }

        if src_path.is_dir() {
            if !recursive {
                println!("[fs] warn: Skipping directory '{}'\n         Use --recursive flag to recursively port directories while maintaining structure", src_path.display());
                continue;
            }
            // Recursively add all files from this directory
            collect_files_recursive(src_path, "", &mut items_to_process)?;
        } else {
            // Single file
            let filename = src_path
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?
                .to_string_lossy()
                .to_string();
            items_to_process.push((src_path.clone(), filename));
        }
    }

    if items_to_process.is_empty() {
        println!("[fs] No items to port");
        return Ok(());
    }

    // Read inode block
    let mut inode_block_buf = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64))?;
    file.read_exact(&mut inode_block_buf)?;
    let mut free_hint = u16::from_le_bytes([inode_block_buf[0], inode_block_buf[1]]);

    // Create necessary directories and port files
    let mut dir_inodes: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    dir_inodes.insert("/".to_string(), 0); // Root is inode 0

    // Track directory data to write later
    let mut dir_data_to_write: std::collections::HashMap<u64, Vec<u8>> = std::collections::HashMap::new();
    dir_data_to_write.insert(0, Vec::new()); // Root data

    // First pass: create all directories and collect their entry data
    if recursive {
        let mut dirs = std::collections::BTreeMap::new();
        for (_src, rel_path) in &items_to_process {
            if let Some(parent) = std::path::Path::new(rel_path).parent() {
                let parent_str = parent.to_string_lossy().to_string();
                if !parent_str.is_empty() && parent_str != "." {
                    dirs.insert(parent_str, ());
                }
            }
        }

        // Sort directories so parents are created before children
        for dir_path in dirs.keys() {
            if !dir_inodes.contains_key(dir_path) {
                // Allocate inode for directory
                if free_hint == 0xFFFF || free_hint as usize >= inodes_per_block {
                    anyhow::bail!("No free inodes available for directory {}", dir_path);
                }

                let dir_inode_idx = free_hint as u64;
                free_hint += 1;

                // Allocate data block for directory
                let data_offset = next_free * BLOCK_SIZE as u64;
                next_free += 1;

                // Create empty directory inode
                let mut dir_inode = vec![0u8; 34];
                dir_inode[0] = 1; // used
                dir_inode[1] = 1; // Directory
                dir_inode[2..10].copy_from_slice(&(BLOCK_SIZE as u64).to_le_bytes());
                dir_inode[10..18].copy_from_slice(&data_offset.to_le_bytes());
                dir_inode[18..26].copy_from_slice(&0u64.to_le_bytes()); // size = 0 initially
                dir_inode[26..34].copy_from_slice(&dir_inode_idx.to_le_bytes()); // index field

                // Write directory inode
                let inode_offset = inode_block_num * BLOCK_SIZE as u64 + 4 + (dir_inode_idx as usize * 34) as u64;
                file.seek(SeekFrom::Start(inode_offset))?;
                file.write_all(&dir_inode)?;

                // Write empty directory data block
                let empty_dir = vec![0u8; BLOCK_SIZE];
                file.seek(SeekFrom::Start(data_offset))?;
                file.write_all(&empty_dir)?;

                println!("[fs] Created directory @/{} -> inode {}", dir_path, dir_inode_idx);
                dir_inodes.insert(dir_path.clone(), dir_inode_idx);
                dir_data_to_write.insert(dir_inode_idx, Vec::new());
            }

            // Add this directory to its parent
            if let Some(parent) = std::path::Path::new(dir_path).parent() {
                let parent_str = parent.to_string_lossy().to_string();
                let parent_key = if parent_str.is_empty() || parent_str == "." {
                    "/".to_string()
                } else {
                    parent_str
                };

                if let Some(parent_inode) = dir_inodes.get(&parent_key) {
                    let dir_name = std::path::Path::new(dir_path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| dir_path.clone());

                    if let Some(parent_data) = dir_data_to_write.get_mut(parent_inode) {
                        let mut entry = Vec::new();
                        entry.extend_from_slice(&(dir_name.len() as u16).to_le_bytes());
                        entry.extend_from_slice(dir_name.as_bytes());
                        entry.extend_from_slice(&dir_inodes[dir_path].to_le_bytes());
                        parent_data.extend_from_slice(&entry);
                    }
                }
            }
        }
    }

    // Second pass: port all files
    for (src_path, rel_path) in items_to_process {
        let metadata = fs::metadata(&src_path)?;
        let file_size = metadata.len();
        
        // Read file content
        let mut file_content = Vec::new();
        let mut src_file = fs::File::open(&src_path)?;
        src_file.read_to_end(&mut file_content)?;

        // Allocate data blocks for file
        let data_offset = next_free * BLOCK_SIZE as u64;
        #[allow(clippy::manual_div_ceil)]
        {
            next_free += (file_size + BLOCK_SIZE as u64 - 1) / BLOCK_SIZE as u64;
        }

        // Write file data
        file.seek(SeekFrom::Start(data_offset))?;
        file.write_all(&file_content)?;

        // Create file inode
        if free_hint == 0xFFFF || free_hint as usize >= inodes_per_block {
            anyhow::bail!("No free inodes available for file {}", rel_path);
        }

        let file_inode_idx = free_hint as u64;
        free_hint += 1;

        let mut file_inode = vec![0u8; 34];
        file_inode[0] = 1; // used
        file_inode[1] = 0; // File
        file_inode[2..10].copy_from_slice(&(file_size).to_le_bytes());
        file_inode[10..18].copy_from_slice(&data_offset.to_le_bytes());
        file_inode[18..26].copy_from_slice(&file_size.to_le_bytes());
        file_inode[26..34].copy_from_slice(&file_inode_idx.to_le_bytes());

        // Write inode
        let inode_offset = inode_block_num * BLOCK_SIZE as u64 + 4 + (file_inode_idx as usize * 34) as u64;
        file.seek(SeekFrom::Start(inode_offset))?;
        file.write_all(&file_inode)?;

        // Get parent directory and add entry
        let parent_dir = if recursive {
            std::path::Path::new(&rel_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .map(|p| if p.is_empty() || p == "." { "/".to_string() } else { p })
                .unwrap_or_else(|| "/".to_string())
        } else {
            "/".to_string()
        };

        let parent_inode_idx = dir_inodes.get(&parent_dir)
            .copied()
            .unwrap_or(0);

        let filename = std::path::Path::new(&rel_path)
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid file path"))?
            .to_string_lossy()
            .to_string();

        let mut entry = Vec::new();
        entry.extend_from_slice(&(filename.len() as u16).to_le_bytes());
        entry.extend_from_slice(filename.as_bytes());
        entry.extend_from_slice(&file_inode_idx.to_le_bytes());

        if let Some(parent_data) = dir_data_to_write.get_mut(&parent_inode_idx) {
            parent_data.extend_from_slice(&entry);
        }

        let path_display = if recursive { format!("@/{}", rel_path) } else { format!("@/{}", filename) };
        println!("[fs] Port: {} ({} bytes) -> inode {}", path_display, file_size, file_inode_idx);
    }

    // Write all directory data and update inodes
    for (dir_inode_idx, dir_data) in dir_data_to_write {
        // Read directory inode
        let mut dir_inode_buf = vec![0u8; 34];
        file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64 + 4 + (dir_inode_idx as usize * 34) as u64))?;
        file.read_exact(&mut dir_inode_buf)?;
        let mut dir_inode = parse_inode(&dir_inode_buf)?;

        // Write directory data
        file.seek(SeekFrom::Start(dir_inode.data_offset))?;
        file.write_all(&dir_data)?;

        // Update inode size
        dir_inode.size = dir_data.len() as u64;
        let mut dir_bytes = vec![0u8; 34];
        dir_bytes[0] = dir_inode.used as u8;
        dir_bytes[1] = dir_inode.kind;
        dir_bytes[2..10].copy_from_slice(&dir_inode.data_size.to_le_bytes());
        dir_bytes[10..18].copy_from_slice(&dir_inode.data_offset.to_le_bytes());
        dir_bytes[18..26].copy_from_slice(&dir_inode.size.to_le_bytes());
        dir_bytes[26..34].copy_from_slice(&dir_inode_idx.to_le_bytes());

        file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64 + 4 + (dir_inode_idx as usize * 34) as u64))?;
        file.write_all(&dir_bytes)?;
    }

    // Update free hint in inode block (only write header, don't overwrite inodes)
    inode_block_buf[0..2].copy_from_slice(&free_hint.to_le_bytes());
    file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64))?;
    file.write_all(&inode_block_buf[0..4])?;

    // Update allocation block with new next_free
    alloc_block[8..16].copy_from_slice(&next_free.to_le_bytes());
    file.seek(SeekFrom::Start(BLOCK_SIZE as u64))?;
    file.write_all(&alloc_block)?;

    file.sync_all()?;
    println!("[fs] File porting completed");
    Ok(())
}

fn collect_files_recursive(dir: &std::path::Path, prefix: &str, result: &mut Vec<(PathBuf, String)>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let filename = entry.file_name();
        let rel_name = format!("{}{}", 
            if prefix.is_empty() { String::new() } else { format!("{}/", prefix) },
            filename.to_string_lossy()
        );

        if path.is_dir() {
            collect_files_recursive(&path, &rel_name, result)?;
        } else {
            result.push((path, rel_name));
        }
    }
    Ok(())
}

/// Normalize a path for output (remove @ prefix, ensure leading /)
fn normalize_path_for_output(path: &str) -> String {
    let path = path.trim_start_matches('@').trim_start_matches('/').trim_end_matches('/');
    if path.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", path)
    }
}

/// Read all inodes from the filesystem
fn read_all_inodes(file: &mut OffsetFile, inode_block_num: u64) -> Result<Vec<(u64, Inode)>> {
    let mut inode_block = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64))?;
    file.read_exact(&mut inode_block)?;

    let inodes_per_block = (BLOCK_SIZE - 4) / 34;
    let mut all_inodes = Vec::new();

    for inode_slot in 0..inodes_per_block {
        let offset = 4 + (inode_slot * 34);
        if offset + 34 > BLOCK_SIZE {
            break;
        }
        
        let used = inode_block[offset] != 0;
        if used {
            let inode = parse_inode(&inode_block[offset..offset+34])?;
            all_inodes.push((inode_slot as u64, inode));
        }
    }

    Ok(all_inodes)
}

/// Navigate to a directory by path and return its inode index
/// Returns the inode index of the directory at the given path
fn navigate_to_dir(
    file: &mut OffsetFile,
    all_inodes: &[(u64, Inode)],
    path: &str,
) -> Result<u64> {
    let path = path.trim_start_matches('@').trim_start_matches('/').trim_end_matches('/');
    
    if path.is_empty() || path == "." {
        return Ok(0); // Root
    }

    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut current_inode_idx = 0u64;

    for part in parts {
        // Read current directory
        if let Some((_, current_inode)) = all_inodes.iter().find(|(idx, _)| *idx == current_inode_idx) {
            match read_directory(file, current_inode) {
                std::result::Result::Ok(entries) => {
                    let mut found = false;
                    for (entry_name, entry_inode) in entries {
                        if entry_name == part.as_bytes() {
                            current_inode_idx = entry_inode;
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        anyhow::bail!("Directory not found: @/{}", path);
                    }
                },
                std::result::Result::Err(_) => {
                    anyhow::bail!("Cannot read directory: @/{}", path);
                }
            }
        } else {
            anyhow::bail!("Inode not found: {}", current_inode_idx);
        }
    }

    std::result::Result::Ok(current_inode_idx)
}

/// Parse a path and return (parent_inode_idx, filename)
fn parse_path(
    file: &mut OffsetFile,
    all_inodes: &[(u64, Inode)],
    path: &str,
) -> Result<(u64, String)> {
    let path = path.trim_start_matches('@').trim_start_matches('/').trim_end_matches('/');
    
    if path.is_empty() {
        anyhow::bail!("Invalid path: expected a file or directory name (e.g., 'docs' or 'docs/file.txt'), got empty path");
    }

    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    
    if parts.is_empty() {
        anyhow::bail!("Invalid path");
    }

    let filename = parts[parts.len() - 1].to_string();
    
    if parts.len() == 1 {
        // Direct child of root
        return Ok((0, filename));
    }

    // Navigate to parent directory
    let parent_path = parts[..parts.len() - 1].join("/");
    let parent_inode = navigate_to_dir(file, all_inodes, &parent_path)?;
    
    Ok((parent_inode, filename))
}

fn fs_mkdir(path: &std::path::Path, partition: Option<u32>, dir_path: &str) -> Result<()> {
    let mut file = OffsetFile::open(path, true, partition)?;

    // Read allocation metadata
    let mut alloc_block = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(BLOCK_SIZE as u64))?;
    file.read_exact(&mut alloc_block)?;

    let mut next_free = u64::from_le_bytes(alloc_block[8..16].try_into()?);
    let inode_block_count = u64::from_le_bytes(alloc_block[16..24].try_into()?);
    
    if inode_block_count == 0 {
        anyhow::bail!("No inode blocks found. Initialize filesystem first.");
    }

    let inode_block_num = u64::from_le_bytes(alloc_block[24..32].try_into()?);
    let inodes_per_block = (BLOCK_SIZE - 4) / 34;

    // Read inode block
    let mut inode_block_buf = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64))?;
    file.read_exact(&mut inode_block_buf)?;
    let mut free_hint = u16::from_le_bytes([inode_block_buf[0], inode_block_buf[1]]);

    if free_hint == 0xFFFF || free_hint as usize >= inodes_per_block {
        anyhow::bail!("No free inodes available");
    }

    // Read all inodes to navigate to parent
    let all_inodes = read_all_inodes(&mut file, inode_block_num)?;

    // Parse path to get parent inode and directory name
    let (parent_inode_idx, dir_name) = parse_path(&mut file, &all_inodes, dir_path)?;

    // Check if directory already exists
    if let Some((_, parent_inode)) = all_inodes.iter().find(|(idx, _)| *idx == parent_inode_idx)
        && let std::result::Result::Ok(entries) = read_directory(&mut file, parent_inode) {
            for (entry_name, _) in entries {
                if entry_name == dir_name.as_bytes() {
                    anyhow::bail!("Directory already exists: @/{}", dir_path);
                }
            }
        }

    let new_inode_idx = free_hint as u64;

    // Allocate data block for directory
    let dir_data_offset = next_free * BLOCK_SIZE as u64;
    next_free += 1;

    // Create directory inode
    let mut dir_inode = vec![0u8; 34];
    dir_inode[0] = 1; // used
    dir_inode[1] = 1; // Directory
    dir_inode[2..10].copy_from_slice(&(BLOCK_SIZE as u64).to_le_bytes()); // data_size
    dir_inode[10..18].copy_from_slice(&dir_data_offset.to_le_bytes()); // data_offset
    dir_inode[18..26].copy_from_slice(&0u64.to_le_bytes()); // size (0 initially)
    dir_inode[26..34].copy_from_slice(&new_inode_idx.to_le_bytes()); // index field

    // Write inode to inode table
    let offset = inode_block_num * BLOCK_SIZE as u64 + 4 + (new_inode_idx as usize * 34) as u64;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&dir_inode)?;

    // Write empty directory data
    let empty_dir = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(dir_data_offset))?;
    file.write_all(&empty_dir)?;

    // Update parent directory
    if let Some((_, mut parent_inode)) = all_inodes.iter().find(|(idx, _)| *idx == parent_inode_idx).map(|(a, b)| (a, b.clone())) {
        let mut parent_data = vec![0u8; parent_inode.size as usize];
        if parent_inode.size > 0 {
            file.seek(SeekFrom::Start(parent_inode.data_offset))?;
            file.read_exact(&mut parent_data)?;
        }

        // Add entry to parent
        let mut entry = Vec::new();
        entry.extend_from_slice(&(dir_name.len() as u16).to_le_bytes());
        entry.extend_from_slice(dir_name.as_bytes());
        entry.extend_from_slice(&new_inode_idx.to_le_bytes());
        parent_data.extend_from_slice(&entry);

        // Write parent data
        file.seek(SeekFrom::Start(parent_inode.data_offset))?;
        file.write_all(&parent_data)?;

        // Update parent inode
        parent_inode.size = parent_data.len() as u64;
        let mut parent_bytes = vec![0u8; 34];
        parent_bytes[0] = parent_inode.used as u8;
        parent_bytes[1] = parent_inode.kind;
        parent_bytes[2..10].copy_from_slice(&parent_inode.data_size.to_le_bytes());
        parent_bytes[10..18].copy_from_slice(&parent_inode.data_offset.to_le_bytes());
        parent_bytes[18..26].copy_from_slice(&parent_inode.size.to_le_bytes());
        parent_bytes[26..34].copy_from_slice(&parent_inode.index.to_le_bytes());

        file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64 + 4 + (parent_inode_idx as usize * 34) as u64))?;
        file.write_all(&parent_bytes)?;
    }

    // Update free hint
    free_hint += 1;
    inode_block_buf[0..2].copy_from_slice(&free_hint.to_le_bytes());
    file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64))?;
    file.write_all(&inode_block_buf[0..4])?;

    // Update allocation block
    alloc_block[8..16].copy_from_slice(&next_free.to_le_bytes());
    file.seek(SeekFrom::Start(BLOCK_SIZE as u64))?;
    file.write_all(&alloc_block)?;

    file.sync_all()?;
    println!("[fs] Created directory: @{} (inode {})", normalize_path_for_output(dir_path), new_inode_idx);
    Ok(())
}

fn fs_touch(path: &std::path::Path, partition: Option<u32>, file_path: &str) -> Result<()> {
    let mut file = OffsetFile::open(path, true, partition)?;

    // Read allocation metadata
    let mut alloc_block = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(BLOCK_SIZE as u64))?;
    file.read_exact(&mut alloc_block)?;

    let mut next_free = u64::from_le_bytes(alloc_block[8..16].try_into()?);
    let inode_block_count = u64::from_le_bytes(alloc_block[16..24].try_into()?);
    
    if inode_block_count == 0 {
        anyhow::bail!("No inode blocks found. Initialize filesystem first.");
    }

    let inode_block_num = u64::from_le_bytes(alloc_block[24..32].try_into()?);
    let inodes_per_block = (BLOCK_SIZE - 4) / 26;

    // Read inode block
    let mut inode_block_buf = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64))?;
    file.read_exact(&mut inode_block_buf)?;

    let mut free_hint = u16::from_le_bytes([inode_block_buf[0], inode_block_buf[1]]);
    if free_hint == 0xFFFF || free_hint as usize >= inodes_per_block {
        anyhow::bail!("No free inodes available");
    }

    // Read all inodes to navigate to parent
    let all_inodes = read_all_inodes(&mut file, inode_block_num)?;

    // Parse path to get parent inode and filename
    let (parent_inode_idx, filename) = parse_path(&mut file, &all_inodes, file_path)?;

    // Check if file already exists
    if let Some((_, parent_inode)) = all_inodes.iter().find(|(idx, _)| *idx == parent_inode_idx)
        && let std::result::Result::Ok(entries) = read_directory(&mut file, parent_inode) {
            for (entry_name, _) in entries {
                if entry_name == filename.as_bytes() {
                    anyhow::bail!("File already exists: @/{}", file_path);
                }
            }
        }

    let new_inode_idx = free_hint as u64;

    // Allocate minimal data block for file
    let file_data_offset = next_free * BLOCK_SIZE as u64;
    next_free += 1;

    // Create file inode
    let mut file_inode = vec![0u8; 34];
    file_inode[0] = 1; // used
    file_inode[1] = 0; // File
    file_inode[2..10].copy_from_slice(&(BLOCK_SIZE as u64).to_le_bytes()); // data_size
    file_inode[10..18].copy_from_slice(&file_data_offset.to_le_bytes()); // data_offset
    file_inode[18..26].copy_from_slice(&0u64.to_le_bytes()); // size (0 initially - empty)
    file_inode[26..34].copy_from_slice(&new_inode_idx.to_le_bytes()); // index field

    // Write inode to inode table
    let offset = inode_block_num * BLOCK_SIZE as u64 + 4 + (new_inode_idx as usize * 34) as u64;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&file_inode)?;

    // Write empty file data block
    let empty_file = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(file_data_offset))?;
    file.write_all(&empty_file)?;

    // Update parent directory
    if let Some((_, mut parent_inode)) = all_inodes.iter().find(|(idx, _)| *idx == parent_inode_idx).map(|(a, b)| (a, b.clone())) {
        let mut parent_data = vec![0u8; parent_inode.size as usize];
        if parent_inode.size > 0 {
            file.seek(SeekFrom::Start(parent_inode.data_offset))?;
            file.read_exact(&mut parent_data)?;
        }

        // Add entry to parent
        let mut entry = Vec::new();
        entry.extend_from_slice(&(filename.len() as u16).to_le_bytes());
        entry.extend_from_slice(filename.as_bytes());
        entry.extend_from_slice(&new_inode_idx.to_le_bytes());
        parent_data.extend_from_slice(&entry);

        // Write parent data
        file.seek(SeekFrom::Start(parent_inode.data_offset))?;
        file.write_all(&parent_data)?;

        // Update parent inode
        parent_inode.size = parent_data.len() as u64;
        let mut parent_bytes = vec![0u8; 34];
        parent_bytes[0] = parent_inode.used as u8;
        parent_bytes[1] = parent_inode.kind;
        parent_bytes[2..10].copy_from_slice(&parent_inode.data_size.to_le_bytes());
        parent_bytes[10..18].copy_from_slice(&parent_inode.data_offset.to_le_bytes());
        parent_bytes[18..26].copy_from_slice(&parent_inode.size.to_le_bytes());
        parent_bytes[26..34].copy_from_slice(&parent_inode.index.to_le_bytes());

        file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64 + 4 + (parent_inode_idx as usize * 34) as u64))?;
        file.write_all(&parent_bytes)?;
    }

    // Update free hint
    free_hint += 1;
    inode_block_buf[0..2].copy_from_slice(&free_hint.to_le_bytes());
    file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64))?;
    file.write_all(&inode_block_buf[0..4])?;

    // Update allocation block
    alloc_block[8..16].copy_from_slice(&next_free.to_le_bytes());
    file.seek(SeekFrom::Start(BLOCK_SIZE as u64))?;
    file.write_all(&alloc_block)?;

    file.sync_all()?;
    println!("[fs] Created file: @{} (inode {})", normalize_path_for_output(file_path), new_inode_idx);
    Ok(())
}

fn fs_rm(path: &std::path::Path, partition: Option<u32>, path_to_remove: &str) -> Result<()> {
    let mut file = OffsetFile::open(path, true, partition)?;

    // Read allocation block
    let mut alloc_block = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(BLOCK_SIZE as u64))?;
    file.read_exact(&mut alloc_block)?;

    let inode_block_count = u64::from_le_bytes(alloc_block[16..24].try_into()?);
    if inode_block_count == 0 {
        anyhow::bail!("No inode blocks found");
    }

    let inode_block_num = u64::from_le_bytes(alloc_block[24..32].try_into()?);
    let inodes_per_block = (BLOCK_SIZE - 4) / 34;

    // Read all inodes to navigate to parent
    let all_inodes = read_all_inodes(&mut file, inode_block_num)?;

    // Parse path to get parent inode and filename
    let (parent_inode_idx, filename) = parse_path(&mut file, &all_inodes, path_to_remove)?;

    // Read parent inode
    let mut parent_inode_buf = vec![0u8; 34];
    file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64 + 4 + (parent_inode_idx as usize * 34) as u64))?;
    file.read_exact(&mut parent_inode_buf)?;
    let parent_inode = parse_inode(&parent_inode_buf)?;

    // Read parent directory data
    let mut parent_data = vec![0u8; parent_inode.size as usize];
    file.seek(SeekFrom::Start(parent_inode.data_offset))?;
    file.read_exact(&mut parent_data)?;

    // Find and remove the entry
    let mut pos = 0;
    let mut found = false;
    let mut target_inode_idx = 0u64;
    let mut removal_start = 0;
    let mut removal_end = 0;

    while pos < parent_data.len() {
        if pos + 2 > parent_data.len() {
            break;
        }

        removal_start = pos;
        let name_len = u16::from_le_bytes([parent_data[pos], parent_data[pos+1]]) as usize;
        pos += 2;

        if pos + name_len + 8 > parent_data.len() {
            break;
        }

        let entry_name = String::from_utf8_lossy(&parent_data[pos..pos+name_len]);
        let entry_inode_idx = u64::from_le_bytes(
            parent_data[pos+name_len..pos+name_len+8].try_into()?
        );

        removal_end = pos + name_len + 8;

        if entry_name.as_ref() == filename {
            target_inode_idx = entry_inode_idx;
            found = true;
            break;
        }

        pos = removal_end;
    }

    if !found {
        anyhow::bail!("File not found: @/{}", path_to_remove);
    }

    // Create new parent data without the removed entry
    let mut new_parent_data = Vec::new();
    if removal_start > 0 {
        new_parent_data.extend_from_slice(&parent_data[0..removal_start]);
    }
    if removal_end < parent_data.len() {
        new_parent_data.extend_from_slice(&parent_data[removal_end..]);
    }

    // Write updated parent directory
    file.seek(SeekFrom::Start(parent_inode.data_offset))?;
    file.write_all(&new_parent_data)?;

    // Update parent inode size
    let mut new_parent_inode = parent_inode_buf.clone();
    new_parent_inode[18..26].copy_from_slice(&(new_parent_data.len() as u64).to_le_bytes());
    file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64 + 4 + (parent_inode_idx as usize * 34) as u64))?;
    file.write_all(&new_parent_inode)?;

    // Mark target inode as unused
    let target_inode_slot = (target_inode_idx % inodes_per_block as u64) as usize;
    let target_inode_offset = inode_block_num * BLOCK_SIZE as u64 + 4 + (target_inode_slot * 34) as u64;
    let mut target_inode = vec![0u8; 34];
    file.seek(SeekFrom::Start(target_inode_offset))?;
    file.read_exact(&mut target_inode)?;

    target_inode[0] = 0; // Mark as unused

    let mut inode_block_buf = vec![0u8; BLOCK_SIZE];
    file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64))?;
    file.read_exact(&mut inode_block_buf)?;

    let free_hint = u16::from_le_bytes([inode_block_buf[0], inode_block_buf[1]]);
    let freed = target_inode_slot as u16;

    if freed < free_hint {
        inode_block_buf[0..2].copy_from_slice(&freed.to_le_bytes());
        file.seek(SeekFrom::Start(inode_block_num * BLOCK_SIZE as u64))?;
        file.write_all(&inode_block_buf[0..4])?;
    }


    file.seek(SeekFrom::Start(target_inode_offset))?;
    file.write_all(&target_inode)?;

    file.sync_all()?;
    println!("[fs] Removed @{} (inode {})", normalize_path_for_output(path_to_remove), target_inode_idx);
    Ok(())
}
