use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

fn main() {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    println!("cargo:rerun-if-changed=linker.ld");
    println!("cargo:rerun-if-changed=linker-aarch64.ld");
    println!("cargo:rerun-if-changed=../user/init.rs");
    println!("cargo:rerun-if-changed=../user/worker.c");
    println!("cargo:rerun-if-changed=../user/linux_compat.c");
    println!("cargo:rerun-if-changed=../user/windows_compat.c");
    println!("cargo:rerun-if-changed=../user/service.c");
    println!("cargo:rerun-if-changed=../user/toolchain.c");
    println!("cargo:rerun-if-changed=../user/dynamic_linker.c");
    println!("cargo:rerun-if-changed=../user/dynamic_app.c");
    println!("cargo:rerun-if-changed=../user/dynamic_lib.c");
    println!("cargo:rerun-if-changed=../sdk/libc/makos.c");
    println!("cargo:rerun-if-changed=../sdk/include/makos.h");
    println!("cargo:rerun-if-changed=../user/linker.ld");
    println!("cargo:rerun-if-changed=../user/aarch64_init.c");
    println!("cargo:rerun-if-changed=../user/aarch64_scheduler.S");
    println!("cargo:rerun-if-changed=../user/aarch64_shell.c");
    println!("cargo:rerun-if-changed=../user/aarch64_toolchain.c");
    println!("cargo:rerun-if-changed=../user/aarch64_smp_probe.S");
    println!("cargo:rerun-if-changed=../user/aarch64_smp_ipc_probe.S");
    println!("cargo:rerun-if-changed=../user/aarch64_smp_exit_group_probe.S");
    println!("cargo:rerun-if-changed=../user/aarch64_textedit.c");
    println!("cargo:rerun-if-changed=../user/aarch64_browser.c");
    println!("cargo:rerun-if-changed=../user/aarch64_files.c");
    println!("cargo:rerun-if-changed=../user/aarch64_textedit_process.c");
    println!("cargo:rerun-if-changed=../user/aarch64_startup_probe.c");
    println!("cargo:rerun-if-changed=../user/aarch64_startup_probe.S");
    println!("cargo:rerun-if-changed=../ports/micropython");
    println!("cargo:rerun-if-changed=../ports/musl");
    println!("cargo:rerun-if-changed=../build/ports/musl/sysroot-shared/usr/lib/libc.so");
    println!("cargo:rerun-if-changed=../user/linker-aarch64.ld");
    if target_arch == "aarch64" {
        build_aarch64_init();
        return;
    }
    if target_arch != "x86_64" {
        return;
    }
    build_init_elf();
    build_c_worker();
    build_linux_compat();
    build_windows_compat();
    build_service();
    build_toolchain();
    build_dynamic_linking();
}

fn build_aarch64_init() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    build_aarch64_micropython(&manifest, &output_dir);
    build_aarch64_musl_probe(&manifest, &output_dir);
    let object = output_dir.join("aarch64-init.o");
    let scheduler_object = output_dir.join("aarch64-scheduler.o");
    let output = output_dir.join("aarch64-init.elf");
    let status = Command::new("clang")
        .args([
            "-target",
            "aarch64-unknown-none-elf",
            "-std=c17",
            "-ffreestanding",
            "-fno-builtin",
            "-fno-stack-protector",
            "-fno-pic",
            "-fno-unwind-tables",
            "-fno-asynchronous-unwind-tables",
            "-mgeneral-regs-only",
            "-Os",
            "-c",
        ])
        .arg(manifest.join("../user/aarch64_init.c"))
        .arg("-o")
        .arg(&object)
        .status()
        .expect("failed to compile AArch64 init fixture");
    assert!(status.success(), "AArch64 init fixture compile failed");
    let status = Command::new("clang")
        .args([
            "-target",
            "aarch64-unknown-none-elf",
            "-ffreestanding",
            "-c",
        ])
        .arg(manifest.join("../user/aarch64_scheduler.S"))
        .arg("-o")
        .arg(&scheduler_object)
        .status()
        .expect("failed to compile AArch64 scheduler fixture");
    assert!(status.success(), "AArch64 scheduler fixture compile failed");
    let status = Command::new(rust_lld())
        .args([
            "-flavor",
            "gnu",
            "--build-id=none",
            "-z",
            "max-page-size=4096",
            "-T",
        ])
        .arg(manifest.join("../user/linker-aarch64.ld"))
        .arg("-o")
        .arg(&output)
        .arg(&object)
        .arg(&scheduler_object)
        .status()
        .expect("failed to link AArch64 init fixture");
    assert!(status.success(), "AArch64 init fixture link failed");

    let shell_object = output_dir.join("aarch64-shell.o");
    let textedit_object = output_dir.join("aarch64-textedit-embedded.o");
    let shell_output = output_dir.join("aarch64-shell.elf");
    let status = Command::new("clang")
        .args([
            "-target",
            "aarch64-unknown-none-elf",
            "-std=c17",
            "-ffreestanding",
            "-fno-builtin",
            "-fno-stack-protector",
            "-fno-pic",
            "-fno-unwind-tables",
            "-fno-asynchronous-unwind-tables",
            "-mgeneral-regs-only",
            "-Os",
            "-c",
        ])
        .arg(manifest.join("../user/aarch64_shell.c"))
        .arg("-o")
        .arg(&shell_object)
        .status()
        .expect("failed to compile AArch64 shell");
    assert!(status.success(), "AArch64 shell compile failed");
    let status = Command::new("clang")
        .args([
            "-target",
            "aarch64-unknown-none-elf",
            "-std=c17",
            "-ffreestanding",
            "-fno-builtin",
            "-fno-stack-protector",
            "-fno-pic",
            "-fno-unwind-tables",
            "-fno-asynchronous-unwind-tables",
            "-mgeneral-regs-only",
            "-Os",
            "-DTEXTEDIT_EMBEDDED",
            "-c",
        ])
        .arg(manifest.join("../user/aarch64_textedit.c"))
        .arg("-o")
        .arg(&textedit_object)
        .status()
        .expect("failed to compile embedded AArch64 Text Edit");
    assert!(
        status.success(),
        "embedded AArch64 Text Edit compile failed"
    );
    let status = Command::new(rust_lld())
        .args([
            "-flavor",
            "gnu",
            "--build-id=none",
            "-z",
            "max-page-size=4096",
            "-T",
        ])
        .arg(manifest.join("../user/linker-aarch64.ld"))
        .arg("-o")
        .arg(&shell_output)
        .arg(&shell_object)
        .arg(&textedit_object)
        .status()
        .expect("failed to link AArch64 shell");
    assert!(status.success(), "AArch64 shell link failed");

    let toolchain_object = output_dir.join("aarch64-toolchain.o");
    let toolchain_output = output_dir.join("aarch64-toolchain.elf");
    let status = Command::new("clang")
        .args([
            "-target",
            "aarch64-unknown-none-elf",
            "-std=c17",
            "-ffreestanding",
            "-fno-builtin",
            "-fno-stack-protector",
            "-fno-pic",
            "-fno-unwind-tables",
            "-fno-asynchronous-unwind-tables",
            "-mgeneral-regs-only",
            "-Os",
            "-c",
        ])
        .arg(manifest.join("../user/aarch64_toolchain.c"))
        .arg("-o")
        .arg(&toolchain_object)
        .status()
        .expect("failed to compile AArch64 guest toolchain");
    assert!(status.success(), "AArch64 guest toolchain compile failed");
    let status = Command::new(rust_lld())
        .args([
            "-flavor",
            "gnu",
            "--build-id=none",
            "-z",
            "max-page-size=4096",
            "-T",
        ])
        .arg(manifest.join("../user/linker-aarch64.ld"))
        .arg("-o")
        .arg(&toolchain_output)
        .arg(&toolchain_object)
        .status()
        .expect("failed to link AArch64 guest toolchain");
    assert!(status.success(), "AArch64 guest toolchain link failed");

    let smp_probe_object = output_dir.join("aarch64-smp-probe.o");
    let smp_probe_output = output_dir.join("aarch64-smp-probe.elf");
    let status = Command::new("clang")
        .args(["-target", "aarch64-unknown-none-elf", "-ffreestanding", "-c"])
        .arg(manifest.join("../user/aarch64_smp_probe.S"))
        .arg("-o")
        .arg(&smp_probe_object)
        .status()
        .expect("failed to compile AArch64 SMP userspace probe");
    assert!(status.success(), "AArch64 SMP userspace probe compile failed");
    let status = Command::new(rust_lld())
        .args([
            "-flavor",
            "gnu",
            "--build-id=none",
            "-z",
            "max-page-size=4096",
            "-T",
        ])
        .arg(manifest.join("../user/linker-aarch64.ld"))
        .arg("-o")
        .arg(&smp_probe_output)
        .arg(&smp_probe_object)
        .status()
        .expect("failed to link AArch64 SMP userspace probe");
    assert!(status.success(), "AArch64 SMP userspace probe link failed");

    let smp_ipc_probe_object = output_dir.join("aarch64-smp-ipc-probe.o");
    let smp_ipc_probe_output = output_dir.join("aarch64-smp-ipc-probe.elf");
    let status = Command::new("clang")
        .args([
            "-target",
            "aarch64-unknown-none-elf",
            "-ffreestanding",
            "-c",
        ])
        .arg(manifest.join("../user/aarch64_smp_ipc_probe.S"))
        .arg("-o")
        .arg(&smp_ipc_probe_object)
        .status()
        .expect("failed to compile AArch64 SMP IPC userspace probe");
    assert!(
        status.success(),
        "AArch64 SMP IPC userspace probe compile failed"
    );
    let status = Command::new(rust_lld())
        .args([
            "-flavor",
            "gnu",
            "--build-id=none",
            "-z",
            "max-page-size=4096",
            "-T",
        ])
        .arg(manifest.join("../user/linker-aarch64.ld"))
        .arg("-o")
        .arg(&smp_ipc_probe_output)
        .arg(&smp_ipc_probe_object)
        .status()
        .expect("failed to link AArch64 SMP IPC userspace probe");
    assert!(
        status.success(),
        "AArch64 SMP IPC userspace probe link failed"
    );

    let smp_exit_group_probe_object = output_dir.join("aarch64-smp-exit-group-probe.o");
    let smp_exit_group_probe_output = output_dir.join("aarch64-smp-exit-group-probe.elf");
    let status = Command::new("clang")
        .args([
            "-target",
            "aarch64-unknown-none-elf",
            "-ffreestanding",
            "-c",
        ])
        .arg(manifest.join("../user/aarch64_smp_exit_group_probe.S"))
        .arg("-o")
        .arg(&smp_exit_group_probe_object)
        .status()
        .expect("failed to compile AArch64 SMP exit-group userspace probe");
    assert!(
        status.success(),
        "AArch64 SMP exit-group userspace probe compile failed"
    );
    let status = Command::new(rust_lld())
        .args([
            "-flavor",
            "gnu",
            "--build-id=none",
            "-z",
            "max-page-size=4096",
            "-T",
        ])
        .arg(manifest.join("../user/linker-aarch64.ld"))
        .arg("-o")
        .arg(&smp_exit_group_probe_output)
        .arg(&smp_exit_group_probe_object)
        .status()
        .expect("failed to link AArch64 SMP exit-group userspace probe");
    assert!(
        status.success(),
        "AArch64 SMP exit-group userspace probe link failed"
    );

    let browser_object = output_dir.join("aarch64-browser.o");
    let browser_output = output_dir.join("aarch64-browser.elf");
    let status = Command::new("clang")
        .args([
            "-target",
            "aarch64-unknown-none-elf",
            "-std=c17",
            "-ffreestanding",
            "-fno-builtin",
            "-fno-stack-protector",
            "-fno-pic",
            "-fno-unwind-tables",
            "-fno-asynchronous-unwind-tables",
            "-mgeneral-regs-only",
            "-Os",
            "-c",
        ])
        .arg(manifest.join("../user/aarch64_browser.c"))
        .arg("-o")
        .arg(&browser_object)
        .status()
        .expect("failed to compile AArch64 Browser");
    assert!(status.success(), "AArch64 Browser compile failed");
    let status = Command::new(rust_lld())
        .args([
            "-flavor",
            "gnu",
            "--build-id=none",
            "-z",
            "max-page-size=4096",
            "-T",
        ])
        .arg(manifest.join("../user/linker-aarch64.ld"))
        .arg("-o")
        .arg(&browser_output)
        .arg(&browser_object)
        .status()
        .expect("failed to link AArch64 Browser");
    assert!(status.success(), "AArch64 Browser link failed");

    for (source, stem, description) in [
        ("aarch64_files.c", "aarch64-files", "AArch64 Files"),
        (
            "aarch64_textedit_process.c",
            "aarch64-textedit",
            "AArch64 isolated Text Edit",
        ),
    ] {
        let object = output_dir.join(format!("{stem}.o"));
        let output = output_dir.join(format!("{stem}.elf"));
        let status = Command::new("clang")
            .args([
                "-target",
                "aarch64-unknown-none-elf",
                "-std=c17",
                "-ffreestanding",
                "-fno-builtin",
                "-fno-stack-protector",
                "-fno-pic",
                "-fno-unwind-tables",
                "-fno-asynchronous-unwind-tables",
                "-mgeneral-regs-only",
                "-Os",
                "-c",
            ])
            .arg(manifest.join("../user").join(source))
            .arg("-o")
            .arg(&object)
            .status()
            .unwrap_or_else(|_| panic!("failed to compile {description}"));
        assert!(status.success(), "{description} compile failed");
        let status = Command::new(rust_lld())
            .args([
                "-flavor",
                "gnu",
                "--build-id=none",
                "-z",
                "max-page-size=4096",
                "-T",
            ])
            .arg(manifest.join("../user/linker-aarch64.ld"))
            .arg("-o")
            .arg(&output)
            .arg(&object)
            .status()
            .unwrap_or_else(|_| panic!("failed to link {description}"));
        assert!(status.success(), "{description} link failed");
    }

    let probe_c = output_dir.join("aarch64-startup-probe-c.o");
    let probe_asm = output_dir.join("aarch64-startup-probe-asm.o");
    let probe_output = output_dir.join("aarch64-startup-probe.elf");
    let status = Command::new("clang")
        .args([
            "-target",
            "aarch64-unknown-none-elf",
            "-std=c17",
            "-ffreestanding",
            "-fno-builtin",
            "-fno-stack-protector",
            "-fno-pic",
            "-fno-unwind-tables",
            "-fno-asynchronous-unwind-tables",
            "-mgeneral-regs-only",
            "-Os",
            "-c",
        ])
        .arg(manifest.join("../user/aarch64_startup_probe.c"))
        .arg("-o")
        .arg(&probe_c)
        .status()
        .expect("failed to compile AArch64 SysV startup probe");
    assert!(
        status.success(),
        "AArch64 SysV startup probe compile failed"
    );
    let status = Command::new("clang")
        .args([
            "-target",
            "aarch64-unknown-none-elf",
            "-ffreestanding",
            "-c",
        ])
        .arg(manifest.join("../user/aarch64_startup_probe.S"))
        .arg("-o")
        .arg(&probe_asm)
        .status()
        .expect("failed to assemble AArch64 SysV startup probe entry");
    assert!(
        status.success(),
        "AArch64 SysV startup probe assembly failed"
    );
    let status = Command::new(rust_lld())
        .args([
            "-flavor",
            "gnu",
            "--build-id=none",
            "-z",
            "max-page-size=4096",
            "-T",
        ])
        .arg(manifest.join("../user/linker-aarch64.ld"))
        .arg("-o")
        .arg(&probe_output)
        .arg(&probe_asm)
        .arg(&probe_c)
        .status()
        .expect("failed to link AArch64 SysV startup probe");
    assert!(status.success(), "AArch64 SysV startup probe link failed");
}

fn build_aarch64_musl_probe(manifest: &std::path::Path, output_dir: &std::path::Path) {
    let workspace = manifest
        .parent()
        .expect("kernel manifest has no workspace parent");
    let port = workspace.join("ports/musl");
    let static_probe = workspace.join("build/ports/musl/makos-static/makos-musl-pthread-probe.elf");
    let shared_probe = workspace.join("build/ports/musl/makos-shared/makos-musl-exec-target.elf");
    if port_rebuild_required(&port, &[&static_probe, &shared_probe]) {
        let status = Command::new(port.join("build-makos.sh"))
            .status()
            .expect("failed to run musl MakOS build");
        assert!(status.success(), "musl MakOS build failed");
        let status = Command::new(port.join("build-shared-makos.sh"))
            .status()
            .expect("failed to run shared musl MakOS build");
        assert!(status.success(), "shared musl MakOS build failed");
    }
    std::fs::copy(
        workspace.join("build/ports/musl/makos-static/makos-musl-probe.elf"),
        output_dir.join("aarch64-musl-probe.elf"),
    )
    .expect("failed to stage musl MakOS runtime probe");
    std::fs::copy(
        workspace.join("build/ports/musl/makos-static/makos-musl-crt-probe.elf"),
        output_dir.join("aarch64-musl-crt-probe.elf"),
    )
    .expect("failed to stage musl MakOS crt runtime probe");
    std::fs::copy(
        workspace.join("build/ports/musl/makos-static/makos-musl-pthread-probe.elf"),
        output_dir.join("aarch64-musl-pthread-probe.elf"),
    )
    .expect("failed to stage musl MakOS pthread runtime probe");
    std::fs::copy(
        workspace.join("build/ports/musl/sysroot-shared/usr/lib/libc.so"),
        output_dir.join("aarch64-musl-loader.so"),
    )
    .expect("failed to stage musl MakOS dynamic loader");
    std::fs::copy(
        workspace.join("build/ports/musl/makos-shared/makos-musl-interp-probe.elf"),
        output_dir.join("aarch64-musl-interp-probe.elf"),
    )
    .expect("failed to stage musl MakOS interpreter probe");
    std::fs::copy(
        workspace.join("build/ports/musl/makos-shared/makos-musl-dynamic-probe.elf"),
        output_dir.join("aarch64-musl-dynamic-probe.elf"),
    )
    .expect("failed to stage musl MakOS shared-libc probe");
    std::fs::copy(
        workspace.join("build/ports/musl/makos-shared/libmakosdemo.so"),
        output_dir.join("aarch64-libmakosdemo.so"),
    )
    .expect("failed to stage MakOS demo shared library");
    std::fs::copy(
        workspace.join("build/ports/musl/makos-shared/makos-musl-dso-probe.elf"),
        output_dir.join("aarch64-musl-dso-probe.elf"),
    )
    .expect("failed to stage musl external-DSO probe");
    std::fs::copy(
        workspace.join("build/ports/musl/makos-shared/makos-musl-dlopen-probe.elf"),
        output_dir.join("aarch64-musl-dlopen-probe.elf"),
    )
    .expect("failed to stage musl dlopen probe");
    std::fs::copy(
        workspace.join("build/ports/musl/makos-shared/makos-musl-exec-caller.elf"),
        output_dir.join("aarch64-musl-exec-caller.elf"),
    )
    .expect("failed to stage musl exec caller");
    std::fs::copy(
        workspace.join("build/ports/musl/makos-shared/makos-musl-exec-target.elf"),
        output_dir.join("aarch64-musl-exec-target.elf"),
    )
    .expect("failed to stage musl exec target");
}

fn port_rebuild_required(source: &Path, artifacts: &[&Path]) -> bool {
    let artifact_times: Vec<_> = artifacts
        .iter()
        .filter_map(|path| std::fs::metadata(path).ok()?.modified().ok())
        .collect();
    if artifact_times.len() != artifacts.len() {
        return true;
    }
    let oldest_artifact = artifact_times.into_iter().min().unwrap();
    newest_file_time(source).is_none_or(|modified| modified > oldest_artifact)
}

fn newest_file_time(path: &Path) -> Option<SystemTime> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.is_file() {
        return metadata.modified().ok();
    }
    let mut newest = SystemTime::UNIX_EPOCH;
    for entry in std::fs::read_dir(path).ok()? {
        let modified = newest_file_time(&entry.ok()?.path())?;
        newest = newest.max(modified);
    }
    Some(newest)
}

fn build_aarch64_micropython(manifest: &std::path::Path, output_dir: &std::path::Path) {
    let workspace = manifest
        .parent()
        .expect("kernel manifest has no workspace parent");
    let destination = workspace.join("build/ports/micropython");
    let status = Command::new(workspace.join("ports/micropython/build-makos.sh"))
        .arg(&destination)
        .status()
        .expect("failed to run MicroPython MakOS build");
    assert!(status.success(), "MicroPython MakOS build failed");
    std::fs::copy(
        destination.join("micropython-makos.elf"),
        output_dir.join("aarch64-python.elf"),
    )
    .expect("failed to stage MicroPython MakOS ELF");
}

fn build_dynamic_linking() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let linker_object = output_dir.join("dynamic-linker.o");
    let application_object = output_dir.join("dynamic-app.o");
    let library_object = output_dir.join("dynamic-lib.o");
    for (source, object, pic) in [
        ("dynamic_linker.c", &linker_object, false),
        ("dynamic_app.c", &application_object, true),
        ("dynamic_lib.c", &library_object, true),
    ] {
        let mut command = Command::new("clang");
        command.args([
            "-target",
            "x86_64-unknown-none-elf",
            "-std=c17",
            "-ffreestanding",
            "-fno-builtin",
            "-fno-stack-protector",
            "-mno-red-zone",
            "-Os",
            "-c",
        ]);
        if pic {
            command.arg("-fPIC");
        } else {
            command.args(["-fno-pic", "-mcmodel=large"]);
        }
        let status = command
            .arg(manifest.join("../user").join(source))
            .arg("-o")
            .arg(object)
            .status()
            .unwrap_or_else(|_| panic!("failed to compile {source}"));
        assert!(status.success(), "dynamic-linking fixture compile failed");
    }

    let linker = rust_lld();
    let loader = output_dir.join("ld-makos.so");
    let status = Command::new(&linker)
        .args(["-flavor", "gnu"])
        .arg("-T")
        .arg(manifest.join("../user/linker.ld"))
        .arg("-o")
        .arg(&loader)
        .arg(&linker_object)
        .status()
        .expect("failed to link MakOS dynamic loader");
    assert!(status.success(), "MakOS dynamic loader link failed");

    let library = output_dir.join("libmakosdemo.so");
    let status = Command::new(&linker)
        .args([
            "-flavor",
            "gnu",
            "-shared",
            "--hash-style=sysv",
            "-z",
            "max-page-size=4096",
            "-soname",
            "libmakosdemo.so",
            "-o",
        ])
        .arg(&library)
        .arg(&library_object)
        .status()
        .expect("failed to link MakOS shared library");
    assert!(status.success(), "MakOS shared library link failed");

    let application = output_dir.join("dynamic-app.elf");
    let status = Command::new(&linker)
        .args([
            "-flavor",
            "gnu",
            "-pie",
            "--entry=_start",
            "--dynamic-linker=/system/ld-makos.so",
            "--hash-style=sysv",
            "--no-as-needed",
            "-z",
            "now",
            "-z",
            "max-page-size=4096",
            "-o",
        ])
        .arg(&application)
        .arg(&application_object)
        .arg(&library)
        .status()
        .expect("failed to link MakOS dynamic application");
    assert!(status.success(), "MakOS dynamic application link failed");
}

fn rust_lld() -> PathBuf {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let sysroot = Command::new(&rustc)
        .args(["--print", "sysroot"])
        .output()
        .expect("failed to locate Rust sysroot");
    assert!(sysroot.status.success(), "rustc sysroot lookup failed");
    let sysroot = String::from_utf8(sysroot.stdout).expect("non-UTF8 Rust sysroot");
    let host = Command::new(&rustc)
        .arg("-vV")
        .output()
        .expect("failed to locate Rust host triple");
    assert!(host.status.success(), "rustc host lookup failed");
    let host = String::from_utf8(host.stdout).expect("non-UTF8 rustc version output");
    let host = host
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("rustc host triple absent");
    PathBuf::from(sysroot.trim())
        .join("lib/rustlib")
        .join(host)
        .join("bin/rust-lld")
}

fn build_toolchain() {
    build_compat_fixture("toolchain", "native toolchain");
}

fn build_service() {
    build_compat_fixture("service", "service");
}

fn build_windows_compat() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let object = output_dir.join("windows_compat.obj");
    let output = output_dir.join("windows_compat.exe");
    let status = Command::new("clang")
        .args([
            "-target",
            "x86_64-pc-windows-msvc",
            "-std=c17",
            "-ffreestanding",
            "-fno-stack-protector",
            "-fno-pic",
            "-mno-red-zone",
            "-mcmodel=large",
            "-Os",
            "-c",
        ])
        .arg(manifest.join("../user/windows_compat.c"))
        .arg("-o")
        .arg(&object)
        .status()
        .expect("failed to compile Windows PE fixture");
    assert!(status.success(), "Windows PE fixture compilation failed");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let sysroot = Command::new(&rustc)
        .args(["--print", "sysroot"])
        .output()
        .expect("failed to locate Rust sysroot");
    assert!(sysroot.status.success(), "rustc sysroot lookup failed");
    let sysroot = String::from_utf8(sysroot.stdout).expect("non-UTF8 Rust sysroot");
    let host_output = Command::new(&rustc)
        .arg("-vV")
        .output()
        .expect("failed to locate Rust host triple");
    assert!(host_output.status.success(), "rustc host lookup failed");
    let host_output = String::from_utf8(host_output.stdout).expect("non-UTF8 rustc version output");
    let host = host_output
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("rustc host triple absent");
    let linker = PathBuf::from(sysroot.trim())
        .join("lib/rustlib")
        .join(host)
        .join("bin/rust-lld");
    let status = Command::new(linker)
        .args([
            "-flavor",
            "link",
            "/entry:_start",
            "/subsystem:console",
            "/nodefaultlib",
            "/fixed",
            "/base:0x100000000",
            "/align:4096",
            "/filealign:512",
        ])
        .arg(format!("/out:{}", output.display()))
        .arg(object)
        .status()
        .expect("failed to link Windows PE fixture");
    assert!(status.success(), "Windows PE fixture link failed");
}

fn build_linux_compat() {
    build_compat_fixture("linux_compat", "Linux");
}

fn build_compat_fixture(name: &str, personality: &str) {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let object = output_dir.join(format!("{name}.o"));
    let output = output_dir.join(format!("{name}.elf"));
    let status = Command::new("clang")
        .args([
            "-target",
            "x86_64-unknown-none-elf",
            "-std=c17",
            "-ffreestanding",
            "-fno-stack-protector",
            "-fno-pic",
            "-mno-red-zone",
            "-mcmodel=large",
            "-Os",
            "-c",
        ])
        .arg(manifest.join(format!("../user/{name}.c")))
        .arg("-o")
        .arg(&object)
        .status()
        .unwrap_or_else(|_| panic!("failed to compile {personality} personality fixture"));
    assert!(
        status.success(),
        "{personality} personality fixture compilation failed"
    );
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let sysroot = Command::new(&rustc)
        .args(["--print", "sysroot"])
        .output()
        .expect("failed to locate Rust sysroot");
    assert!(sysroot.status.success(), "rustc sysroot lookup failed");
    let sysroot = String::from_utf8(sysroot.stdout).expect("non-UTF8 Rust sysroot");
    let host_output = Command::new(&rustc)
        .arg("-vV")
        .output()
        .expect("failed to locate Rust host triple");
    assert!(host_output.status.success(), "rustc host lookup failed");
    let host_output = String::from_utf8(host_output.stdout).expect("non-UTF8 rustc version output");
    let host = host_output
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("rustc host triple absent");
    let linker = PathBuf::from(sysroot.trim())
        .join("lib/rustlib")
        .join(host)
        .join("bin/rust-lld");
    let status = Command::new(linker)
        .args(["-flavor", "gnu"])
        .arg("-T")
        .arg(manifest.join("../user/linker.ld"))
        .arg("-o")
        .arg(output)
        .arg(object)
        .status()
        .unwrap_or_else(|_| panic!("failed to link {personality} personality fixture"));
    assert!(
        status.success(),
        "{personality} personality fixture link failed"
    );
}

fn build_c_worker() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let include = manifest.join("../sdk/include");
    let worker_object = output_dir.join("worker.o");
    let libc_object = output_dir.join("makos-libc.o");
    let output = output_dir.join("worker.elf");
    for (source, object) in [
        (manifest.join("../user/worker.c"), &worker_object),
        (manifest.join("../sdk/libc/makos.c"), &libc_object),
    ] {
        let status = Command::new("clang")
            .args([
                "-target",
                "x86_64-unknown-none-elf",
                "-std=c17",
                "-ffreestanding",
                "-fno-stack-protector",
                "-fno-pic",
                "-mno-red-zone",
                "-mcmodel=large",
                "-Os",
                "-c",
            ])
            .arg(format!("-I{}", include.display()))
            .arg(source)
            .arg("-o")
            .arg(object)
            .status()
            .expect("failed to compile MakOS C userspace");
        assert!(status.success(), "MakOS C userspace compilation failed");
    }
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let sysroot = Command::new(&rustc)
        .args(["--print", "sysroot"])
        .output()
        .expect("failed to locate Rust sysroot");
    assert!(sysroot.status.success(), "rustc sysroot lookup failed");
    let sysroot = String::from_utf8(sysroot.stdout).expect("non-UTF8 Rust sysroot");
    let host = Command::new(&rustc)
        .arg("-vV")
        .output()
        .expect("failed to locate Rust host triple");
    assert!(host.status.success(), "rustc host lookup failed");
    let host = String::from_utf8(host.stdout).expect("non-UTF8 rustc version output");
    let host = host
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("rustc host triple absent");
    let linker = PathBuf::from(sysroot.trim())
        .join("lib/rustlib")
        .join(host)
        .join("bin/rust-lld");
    let status = Command::new(linker)
        .args(["-flavor", "gnu"])
        .arg("-T")
        .arg(manifest.join("../user/linker.ld"))
        .arg("-o")
        .arg(output)
        .arg(worker_object)
        .arg(libc_object)
        .status()
        .expect("failed to link MakOS C userspace");
    assert!(status.success(), "MakOS C userspace link failed");
}

fn build_init_elf() {
    build_user_elf("init");
}

fn build_user_elf(name: &str) {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join(format!("{name}.elf"));
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(rustc)
        .arg(manifest.join(format!("../user/{name}.rs")))
        .arg("--edition=2024")
        .arg("--target=x86_64-unknown-none")
        .arg("-Cpanic=abort")
        .arg("-Crelocation-model=static")
        .arg("-Ccode-model=large")
        .arg(format!(
            "-Clink-arg=-T{}",
            manifest.join("../user/linker.ld").display()
        ))
        .arg("-o")
        .arg(output)
        .status()
        .unwrap_or_else(|_| panic!("failed to run rustc for userspace {name}"));
    assert!(status.success(), "userspace {name} compilation failed");
}
