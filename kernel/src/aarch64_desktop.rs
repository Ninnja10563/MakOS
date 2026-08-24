use alloc::format;
use makos_boot_api::FramebufferInfo;

pub fn initialize(framebuffer: FramebufferInfo) {
    crate::graphics::init(framebuffer);
    crate::graphics::show_login();
    crate::aarch64_virtio_input::init();
}

pub fn execute_command(command: &[u8], terminal: u64) {
    if command.starts_with(&[0xff, b'A']) {
        execute_add_user(command);
        return;
    }
    match command {
        b"" => {}
        b"help" => crate::graphics::terminal_write(
            b"help status mem ps clear pwd ls ls -l cat stat touch write cp mv rm wc edit nano python abi-startup musl-probe musl-crt echo whoami uname uptime adduser install signout exit\n",
        ),
        b"status" => {
            let line = format!(
                "kernel=online arch=aarch64 el=1 free_frames={} uptime_ms={}\n",
                crate::mm::free_frames(),
                crate::arch::uptime_millis(),
            );
            crate::graphics::terminal_write(line.as_bytes());
        }
        b"clear" => crate::graphics::terminal_clear(),
        b"mem" => {
            let free = crate::mm::free_frames();
            output(format!("free_frames={free} free_kib={}\n", free * 4));
            crate::serial_println!("MAKOS_AARCH64_SHELL_CMD mem free_frames={}", free);
        }
        b"ps" => {
            let stats = crate::aarch64_process::runtime_stats();
            output(format!(
                "tasks={} runnable={} blocked={} zombies={} current={}\n",
                stats.live, stats.runnable, stats.blocked, stats.zombies, stats.current_pid
            ));
            crate::aarch64_process::report_runtime_tasks();
            crate::serial_println!(
                "MAKOS_AARCH64_SHELL_CMD ps live={} runnable={} blocked={} zombies={} current={}",
                stats.live,
                stats.runnable,
                stats.blocked,
                stats.zombies,
                stats.current_pid,
            );
        }
        b"pwd" => {
            let mut username = [0u8; makos_accounts::USERNAME_BYTES];
            if let Some(length) = crate::security::session_username(&mut username) {
                crate::graphics::terminal_write(b"/home/");
                crate::graphics::terminal_write(&username[..length]);
                crate::graphics::terminal_write(b"\n");
            }
        }
        b"ls" | b"ls /home/user" => shell_list(false),
        b"ls -l" | b"ls -l /home/user" => shell_list(true),
        b"cat note.txt" | b"cat /home/user/note.txt" => shell_cat(b"note.txt"),
        value if value.starts_with(b"cat ") => shell_cat(&value[4..]),
        value if value.starts_with(b"stat ") => shell_stat(&value[5..]),
        value if value.starts_with(b"touch ") => shell_touch(&value[6..]),
        value if value.starts_with(b"write ") => shell_write(&value[6..]),
        value if value.starts_with(b"cp ") => shell_copy(&value[3..], false),
        value if value.starts_with(b"mv ") => shell_copy(&value[3..], true),
        value if value.starts_with(b"rm ") => shell_remove(&value[3..]),
        value if value.starts_with(b"wc ") => shell_word_count(&value[3..]),
        b"whoami" => {
            let mut username = [0u8; makos_accounts::USERNAME_BYTES];
            if let Some(length) = crate::security::session_username(&mut username) {
                crate::graphics::terminal_write(&username[..length]);
                crate::graphics::terminal_write(b"\n");
                crate::serial_println!(
                    "MAKOS_AARCH64_SHELL_CMD whoami user={}",
                    core::str::from_utf8(&username[..length]).unwrap_or("invalid")
                );
            }
        }
        b"uname" | b"uname -a" => {
            crate::graphics::terminal_write(b"MakOS 0.1.0 aarch64 makos\n")
        }
        b"uptime" => {
            let line = format!("{} ms\n", crate::arch::uptime_millis());
            crate::graphics::terminal_write(line.as_bytes());
        }
        b"install" => crate::graphics::terminal_write(
            b"usage: install disk1 erase-disk1 | install disk1 resume-disk1\nWARNING: fresh target must be blank; resume requires source-identical partial data.\n",
        ),
        b"install disk1 erase-disk1" => {
            crate::graphics::terminal_write(
                b"Installing disk0 to blank disk1. UI pauses during verified copy...\n",
            );
            match crate::aarch64_installer::install_disk1(makos_installer::InstallMode::Fresh) {
                Ok(report) => crate::aarch64_installer::success_message(report),
                Err(error) => crate::aarch64_installer::describe_error(error),
            }
        }
        b"install disk1 resume-disk1" => {
            crate::graphics::terminal_write(
                b"Resuming source-matching disk0 copy to disk1. UI pauses during verified copy...\n",
            );
            match crate::aarch64_installer::install_disk1(makos_installer::InstallMode::Resume) {
                Ok(report) => crate::aarch64_installer::success_message(report),
                Err(error) => crate::aarch64_installer::describe_error(error),
            }
        }
        value if value.starts_with(b"install ") => {
            crate::graphics::terminal_write(
                b"install: confirmation mismatch; type exactly: install disk1 erase-disk1 or install disk1 resume-disk1\n",
            );
            crate::serial_println!(
                "MAKOS_INSTALL_CONFIRMATION_DENIED target=disk1 expected=erase-disk1 destructive_io=0"
            );
        }
        b"exit" => {
            crate::graphics::terminal_write(b"Terminal closed. Reopen from Start.\n");
            if crate::graphics::close(terminal) {
                crate::serial_println!(
                    "MAKOS_AARCH64_TERMINAL_EXIT_OK close=1 reopen=start-menu state=retained"
                );
            }
        }
        b"signout" => {
            if !crate::security::sign_out() {
                crate::graphics::terminal_write(b"signout failed\n");
            }
        }
        value if value.starts_with(b"echo ") => {
            crate::graphics::terminal_write(&value[5..]);
            crate::graphics::terminal_write(b"\n");
            crate::serial_println!(
                "MAKOS_AARCH64_SHELL_INPUT_OK exact={} lowercase=1 punctuation=1",
                core::str::from_utf8(&value[5..]).unwrap_or("invalid-utf8"),
            );
        }
        _ => crate::graphics::terminal_write(b"command not found\n"),
    }
}

fn execute_add_user(command: &[u8]) {
    if command.len() < 4 {
        crate::graphics::terminal_write(b"adduser: malformed request\n");
        return;
    }
    let username_length = usize::from(command[2]);
    let password_length = usize::from(command[3]);
    if username_length > makos_accounts::USERNAME_BYTES
        || password_length > makos_accounts::PASSWORD_BYTES
        || command.len() != 4 + username_length + password_length
    {
        crate::graphics::terminal_write(b"adduser: malformed request\n");
        return;
    }
    let username = &command[4..4 + username_length];
    let password = &command[4 + username_length..];
    match crate::security::add_user(username, password) {
        Ok((uid, gid)) => {
            let name = core::str::from_utf8(username).unwrap_or("invalid");
            output(format!("user {name} created (uid={uid}, gid={gid})\n"));
            crate::serial_println!(
                "MAKOS_ADDUSER_OK user={} uid={} gid={} persisted=makfs-vfs password=pbkdf2-hmac-sha256 plaintext=never-stored",
                name,
                uid,
                gid,
            );
        }
        Err(crate::security::AddUserError::InvalidUsername) => crate::graphics::terminal_write(
            b"adduser: username must start with lowercase letter; use lowercase letters, digits, _ or -\n",
        ),
        Err(crate::security::AddUserError::InvalidPassword) => {
            crate::graphics::terminal_write(b"adduser: password must be 8-64 characters\n")
        }
        Err(crate::security::AddUserError::Exists) => {
            crate::graphics::terminal_write(b"adduser: user already exists\n")
        }
        Err(crate::security::AddUserError::Full) => {
            crate::graphics::terminal_write(b"adduser: account database full\n")
        }
        Err(crate::security::AddUserError::Storage) => {
            crate::graphics::terminal_write(b"adduser: persistent storage failed\n")
        }
        Err(crate::security::AddUserError::Permission) => {
            crate::graphics::terminal_write(b"adduser: permission denied\n")
        }
    }
}

fn shell_list(long: bool) {
    let mut index = 0usize;
    while let Some(entry) = crate::vfs::read_dir(b"/home/user", index) {
        let length = entry.name_length as usize;
        if long {
            let mut path_buffer = [0u8; 64];
            if let Some(path) = shell_path(&entry.name[..length], &mut path_buffer) {
                if let Some(metadata) = crate::vfs::stat(path) {
                    output(format!(
                        "{:04o} {:>4} {:>4} {:>5} ",
                        metadata.mode, metadata.uid, metadata.gid, metadata.size,
                    ));
                }
            }
        }
        crate::graphics::terminal_write(&entry.name[..length]);
        crate::graphics::terminal_write(b"\n");
        index += 1;
    }
    crate::serial_println!(
        "MAKOS_AARCH64_SHELL_CMD ls entries={} long={}",
        index,
        u8::from(long),
    );
}

fn shell_cat(name: &[u8]) {
    let mut path = [0u8; 64];
    let Some(path) = shell_path(name, &mut path) else {
        crate::graphics::terminal_write(b"cat: invalid path\n");
        return;
    };
    let mut contents = [0u8; crate::vfs::MAX_FILE_BYTES];
    let Some(count) = crate::vfs::snapshot(path, &mut contents) else {
        crate::graphics::terminal_write(b"cat: file not found\n");
        crate::serial_println!(
            "MAKOS_AARCH64_SHELL_CAT_ERROR reason=not-found path={}",
            core::str::from_utf8(path).unwrap_or("invalid-utf8"),
        );
        return;
    };
    crate::graphics::terminal_write(&contents[..count]);
    if count == 0 || contents[count - 1] != b'\n' {
        crate::graphics::terminal_write(b"\n");
    }
    crate::serial_println!(
        "MAKOS_AARCH64_SHELL_CMD cat bytes={} path={}",
        count,
        core::str::from_utf8(path).unwrap_or("invalid-utf8"),
    );
}

fn shell_stat(name: &[u8]) {
    let mut path = [0u8; 64];
    let Some(path) = shell_path(name, &mut path) else {
        crate::graphics::terminal_write(b"stat: invalid path\n");
        return;
    };
    let Some(metadata) = crate::vfs::stat(path) else {
        crate::graphics::terminal_write(b"stat: file not found\n");
        return;
    };
    output(format!(
        "size={} uid={} gid={} mode={:o} inode={} modified_ticks={}\n",
        metadata.size,
        metadata.uid,
        metadata.gid,
        metadata.mode,
        metadata.inode,
        metadata.modified_ticks,
    ));
    crate::serial_println!(
        "MAKOS_AARCH64_SHELL_CMD stat size={} inode={}",
        metadata.size,
        metadata.inode,
    );
}

fn shell_touch(name: &[u8]) {
    let mut path = [0u8; 64];
    let Some(path) = shell_path(name, &mut path) else {
        crate::graphics::terminal_write(b"touch: invalid path\n");
        return;
    };
    if !crate::vfs::create(path) {
        crate::graphics::terminal_write(b"touch: create failed (file may exist)\n");
        return;
    }
    crate::serial_println!("MAKOS_AARCH64_SHELL_CMD touch persisted=1");
}

fn shell_write(arguments: &[u8]) {
    let Some(separator) = arguments.iter().position(|byte| *byte == b' ') else {
        crate::graphics::terminal_write(b"usage: write FILE TEXT\n");
        return;
    };
    let name = &arguments[..separator];
    let contents = &arguments[separator + 1..];
    let mut path = [0u8; 64];
    let Some(path) = shell_path(name, &mut path) else {
        crate::graphics::terminal_write(b"write: invalid path\n");
        return;
    };
    if crate::vfs::stat(path).is_none() && !crate::vfs::create(path) {
        crate::graphics::terminal_write(b"write: create failed\n");
        return;
    }
    let Some(fd) = crate::vfs::open(path, true) else {
        crate::graphics::terminal_write(b"write: open failed\n");
        return;
    };
    let written = crate::vfs::write(fd, contents);
    let closed = crate::vfs::close(fd);
    if written != Some(contents.len()) || !closed {
        crate::graphics::terminal_write(b"write: persistence failed\n");
        return;
    }
    crate::serial_println!(
        "MAKOS_AARCH64_SHELL_CMD write bytes={} persisted=1",
        contents.len(),
    );
}

fn shell_copy(arguments: &[u8], remove_source: bool) {
    let Some(separator) = arguments.iter().position(|byte| *byte == b' ') else {
        crate::graphics::terminal_write(if remove_source {
            b"usage: mv SOURCE DEST\n"
        } else {
            b"usage: cp SOURCE DEST\n"
        });
        return;
    };
    let source_name = &arguments[..separator];
    let destination_name = arguments[separator + 1..]
        .split(|byte| *byte == b' ')
        .next()
        .unwrap_or_default();
    if source_name.is_empty() || destination_name.is_empty() || source_name == destination_name {
        crate::graphics::terminal_write(b"copy: invalid source/destination\n");
        return;
    }
    let mut source_buffer = [0u8; 64];
    let mut destination_buffer = [0u8; 64];
    let Some(source) = shell_path(source_name, &mut source_buffer) else {
        crate::graphics::terminal_write(b"copy: invalid source path\n");
        return;
    };
    let Some(destination) = shell_path(destination_name, &mut destination_buffer) else {
        crate::graphics::terminal_write(b"copy: invalid destination path\n");
        return;
    };
    if remove_source {
        if !crate::vfs::rename(source, destination) {
            crate::graphics::terminal_write(b"mv: rename failed\n");
            return;
        }
        crate::serial_println!("MAKOS_AARCH64_SHELL_CMD mv vfs=real rename=atomic persisted=1");
        return;
    }
    let mut contents = [0u8; crate::vfs::MAX_FILE_BYTES];
    let Some(count) = crate::vfs::snapshot(source, &mut contents) else {
        crate::graphics::terminal_write(b"copy: source not found\n");
        return;
    };
    if crate::vfs::stat(destination).is_none() && !crate::vfs::create(destination) {
        crate::graphics::terminal_write(b"copy: destination create failed\n");
        return;
    }
    let Some(fd) = crate::vfs::open(destination, true) else {
        crate::graphics::terminal_write(b"copy: destination open failed\n");
        return;
    };
    let written = crate::vfs::write(fd, &contents[..count]);
    let closed = crate::vfs::close(fd);
    if written != Some(count) || !closed {
        crate::graphics::terminal_write(b"copy: destination write failed\n");
        return;
    }
    crate::serial_println!(
        "MAKOS_AARCH64_SHELL_CMD {} bytes={} vfs=real persisted=1",
        "cp",
        count,
    );
}

fn shell_word_count(name: &[u8]) {
    let mut path_buffer = [0u8; 64];
    let Some(path) = shell_path(name, &mut path_buffer) else {
        crate::graphics::terminal_write(b"wc: invalid path\n");
        return;
    };
    let mut contents = [0u8; crate::vfs::MAX_FILE_BYTES];
    let Some(count) = crate::vfs::snapshot(path, &mut contents) else {
        crate::graphics::terminal_write(b"wc: file not found\n");
        return;
    };
    let lines = contents[..count]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count();
    let mut words = 0usize;
    let mut in_word = false;
    for byte in &contents[..count] {
        let whitespace = matches!(*byte, b' ' | b'\t' | b'\r' | b'\n');
        if !whitespace && !in_word {
            words += 1;
        }
        in_word = !whitespace;
    }
    output(format!("{} {} {} ", lines, words, count));
    crate::graphics::terminal_write(name);
    crate::graphics::terminal_write(b"\n");
    crate::serial_println!(
        "MAKOS_AARCH64_SHELL_CMD wc lines={} words={} bytes={} vfs=real",
        lines,
        words,
        count,
    );
}

fn shell_remove(name: &[u8]) {
    let mut path = [0u8; 64];
    let Some(path) = shell_path(name, &mut path) else {
        crate::graphics::terminal_write(b"rm: invalid path\n");
        return;
    };
    if !crate::vfs::unlink(path) {
        crate::graphics::terminal_write(b"rm: remove failed\n");
        return;
    }
    crate::serial_println!("MAKOS_AARCH64_SHELL_CMD rm persisted=1");
}

fn shell_path<'a>(name: &[u8], output: &'a mut [u8; 64]) -> Option<&'a [u8]> {
    if name.starts_with(b"/") {
        if name.len() > output.len() {
            return None;
        }
        output[..name.len()].copy_from_slice(name);
        return Some(&output[..name.len()]);
    }
    if name.is_empty() || name.len() > 32 {
        return None;
    }
    const PREFIX: &[u8] = b"/home/user/";
    output[..PREFIX.len()].copy_from_slice(PREFIX);
    output[PREFIX.len()..PREFIX.len() + name.len()].copy_from_slice(name);
    Some(&output[..PREFIX.len() + name.len()])
}

fn output(value: impl AsRef<str>) {
    crate::graphics::terminal_write(value.as_ref().as_bytes());
}
