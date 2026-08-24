use std::path::PathBuf;

fn main() {
    let mut arguments = std::env::args_os();
    let program = arguments.next().unwrap_or_default();
    let Some(path) = arguments.next().map(PathBuf::from) else {
        eprintln!("usage: {} IMAGE", PathBuf::from(program).display());
        std::process::exit(2);
    };
    if arguments.next().is_some() {
        eprintln!("expected exactly one image path");
        std::process::exit(2);
    }
    match makos_makfs4_fsck::check_path(&path) {
        Ok(report) => println!(
            "MAKOS_MAKFS4_FSCK_OK generation={} root_slot={} volume_offset={} inodes={} files={} directories={} symlinks={} allocated_blocks={}",
            report.generation,
            report.root_slot,
            report.volume_offset_bytes,
            report.inodes,
            report.files,
            report.directories,
            report.symlinks,
            report.allocated_blocks,
        ),
        Err(error) => {
            eprintln!(
                "MAKOS_MAKFS4_FSCK_FAIL image={} error={error}",
                path.display()
            );
            std::process::exit(1);
        }
    }
}
