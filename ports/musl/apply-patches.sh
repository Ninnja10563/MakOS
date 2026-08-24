#!/bin/sh
# SPDX-License-Identifier: MIT
set -eu

port_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_dir=$(CDPATH= cd -- "$port_dir/../.." && pwd)
source_dir=${MUSL_SOURCE_DIR:-"$repo_dir/build/ports/musl/source"}
. "$port_dir/source.lock"

apply_final_fixes()
{
	if ! grep -q '#include <netinet/in.h>' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0034-makos-ipv4-types.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_thread_set_name = 115' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0035-makos-thread-names.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_thread_set_scheduler = 116' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0036-makos-sched-other.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'd != MAP_SHARED' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0037-makos-posix-shmem.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_compat_missing = 118' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0038-makos-missing-syscall-trace.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'case L_writev:' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0039-makos-writev.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_process_exit = 119' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0040-makos-exit-group.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'L_membarrier = 283' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0041-makos-firefox-platform-syscalls.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_mremap_probe = 120' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0042-makos-mremap-probe.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'L_readlinkat = 78' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0043-makos-readlinkat-probe.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_filesystem_stats = 121' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0044-makos-statfs.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_uname = 122' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0045-makos-uname.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'L_sched_getparam = 121' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0046-makos-sched-getparam.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_socketpair = 124' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0047-makos-socketpair.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_process_fork = 125' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0048-makos-process-fork.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -Eq 'if \(!message \|\| message->msg_iovlen > (16|IOV_MAX)\)$' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0049-makos-recvmsg-control-buffer.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'case L_readv:' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0050-makos-readv.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_process_wait_status = 126' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0051-makos-waitid.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_recvmsg_rights = 128' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0052-makos-scm-rights.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'msg_iovlen > IOV_MAX' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0053-makos-iov-max.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'L_sched_getscheduler = 120' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0054-makos-scheduler-advice.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'if (total == sizeof buffer) break;' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0055-makos-sendmsg-short-write.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'L_fchmodat = 53' "$source_dir/src/internal/makos_syscall.c" \
		|| ! grep -q 'case L_fchmodat:' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0056-makos-idempotent-fchmodat.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'static __thread unsigned char buffer\[65536\]' \
		"$source_dir/src/internal/makos_syscall.c" \
		&& ! grep -q 'if (!message->msg_controllen && message->msg_iovlen == 1)' \
		"$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0057-makos-message-buffer-tls.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'message->msg_iovlen == 1' \
		"$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0058-makos-recvmsg-single-iov-direct.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'if (!message->msg_controllen && message->msg_iovlen == 1)' \
		"$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0059-makos-message-buffer-stack-bounds.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_socket_name = 134' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0060-makos-ipv6-sockets.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'IPV6_V6ONLY' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0061-makos-ipv6-v6only.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_stat_extended = 132' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0062-makos-symlinks-timestamps.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_robust_list = 141' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0063-makos-robust-futex.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'M_signal = 142' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0064-makos-directed-signals.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
}

test -d "$source_dir/.git" || {
	echo "musl source absent: run ports/musl/clone.sh" >&2
	exit 1
}
test "$(git -C "$source_dir" rev-parse HEAD)" = "$MUSL_COMMIT" || {
	echo "musl source revision mismatch" >&2
	exit 1
}

# Build entrypoints request a clean replay so edits to an already-applied
# patch cannot leave generated musl sources on an older ABI.
if test "${MUSL_REAPPLY_PATCHES-0}" = 1; then
	git -C "$source_dir" reset --hard "$MUSL_COMMIT" >/dev/null
	git -C "$source_dir" clean -fd >/dev/null
fi

complete_series()
{
	for patch in \
		"$port_dir/patches/0014-makos-truncate-sync.patch" \
		"$port_dir/patches/0015-makos-directories.patch" \
		"$port_dir/patches/0016-makos-positional-io.patch" \
		"$port_dir/patches/0017-makos-sleep-clocks.patch" \
		"$port_dir/patches/0018-makos-blocking-pipe-poll.patch" \
		"$port_dir/patches/0019-makos-epoll.patch" \
		"$port_dir/patches/0020-makos-signal-masks.patch" \
		"$port_dir/patches/0021-makos-pselect.patch" \
		"$port_dir/patches/0022-makos-private-file-mmap.patch" \
		"$port_dir/patches/0023-makos-execve.patch" \
		"$port_dir/patches/0024-makos-map-noreserve.patch" \
		"$port_dir/patches/0025-makos-socket-flags.patch" \
		"$port_dir/patches/0026-makos-socket-options.patch" \
		"$port_dir/patches/0027-makos-socket-names.patch" \
		"$port_dir/patches/0028-makos-dns-sockets.patch" \
		"$port_dir/patches/0029-makos-dns-config.patch" \
		"$port_dir/patches/0030-makos-madvise.patch" \
		"$port_dir/patches/0031-makos-getppid.patch" \
		"$port_dir/patches/0032-makos-rdwr.patch" \
		"$port_dir/patches/0033-makos-record-locks.patch" \
		"$port_dir/patches/0034-makos-ipv4-types.patch"
	do
		if git -C "$source_dir" apply --check "$patch" 2>/dev/null; then
			git -C "$source_dir" apply "$patch"
		elif ! git -C "$source_dir" apply --reverse --check "$patch" 2>/dev/null; then
			echo "musl patch state invalid: $patch" >&2
			exit 1
		fi
	done
	git -C "$source_dir" diff --check
	apply_final_fixes
	echo "MAKOS_MUSL_PATCHES_OK revision=$MUSL_COMMIT patches=64"
	exit 0
}

# Later patches intentionally edit context introduced by earlier patches, so
# checking each earlier patch in reverse after the full series landed is not
# valid. Recognize full/partial ordered series by terminal ABI symbols.
if test -f "$source_dir/src/internal/makos_syscall.c"; then
if grep -q 'M_pselect = 109' "$source_dir/src/internal/makos_syscall.c" \
	&& grep -q 'SYS_thread_clone' "$source_dir/src/thread/aarch64/clone.s" \
	&& grep -Fq 'd != (MAP_PRIVATE | MAP_FIXED)' "$source_dir/src/internal/makos_syscall.c"; then
	if ! grep -q 'M_execve = 112' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0023-makos-execve.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'd &= ~MAP_NORESERVE' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0024-makos-map-noreserve.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'SOCK_NONBLOCK | SOCK_CLOEXEC' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0025-makos-socket-flags.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'L_getsockopt = 209' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0026-makos-socket-options.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'L_getpeername = 205' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0027-makos-socket-names.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'L_recvmsg = 212' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0028-makos-dns-sockets.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q '__makos_get_net_config' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0029-makos-dns-config.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'L_madvise = 233' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0030-makos-madvise.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'L_getppid = 173' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0031-makos-getppid.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if grep -q 'if (access == O_RDWR) return -ENOTSUP;' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0032-makos-rdwr.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	if ! grep -q 'case F_SETLKW:' "$source_dir/src/internal/makos_syscall.c"; then
		patch="$port_dir/patches/0033-makos-record-locks.patch"
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	fi
	apply_final_fixes
	git -C "$source_dir" diff --check
	echo "MAKOS_MUSL_PATCHES_OK revision=$MUSL_COMMIT patches=64"
	exit 0
fi

if grep -q 'M_sigprocmask = 108' "$source_dir/src/internal/makos_syscall.c" \
	&& grep -q 'SYS_thread_clone' "$source_dir/src/thread/aarch64/clone.s"; then
	patch="$port_dir/patches/0021-makos-pselect.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0022-makos-private-file-mmap.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0023-makos-execve.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0024-makos-map-noreserve.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0025-makos-socket-flags.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0026-makos-socket-options.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0027-makos-socket-names.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0028-makos-dns-sockets.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0029-makos-dns-config.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0030-makos-madvise.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0031-makos-getppid.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0032-makos-rdwr.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0033-makos-record-locks.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	apply_final_fixes
	git -C "$source_dir" diff --check
		echo "MAKOS_MUSL_PATCHES_OK revision=$MUSL_COMMIT patches=64"
	exit 0
fi

if grep -q 'M_epoll_close = 107' "$source_dir/src/internal/makos_syscall.c" \
	&& grep -q 'SYS_thread_clone' "$source_dir/src/thread/aarch64/clone.s"; then
	patch="$port_dir/patches/0020-makos-signal-masks.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	complete_series
fi

if grep -q 'M_pwrite = 101' "$source_dir/src/internal/makos_syscall.c"; then
	complete_series
fi

if grep -q 'M_rmdir = 99' "$source_dir/src/internal/makos_syscall.c"; then
	complete_series
fi

if grep -q 'M_fsync = 97' "$source_dir/src/internal/makos_syscall.c"; then
	complete_series
fi

if grep -q 'M_getcwd = 95' "$source_dir/src/internal/makos_syscall.c"; then
	complete_series
fi

if grep -q 'M_read_dir_fd = 93' "$source_dir/src/internal/makos_syscall.c"; then
	patch="$port_dir/patches/0013-makos-working-directory.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	complete_series
fi

if grep -q 'M_fstat_metadata = 92' "$source_dir/src/internal/makos_syscall.c"; then
	patch="$port_dir/patches/0012-makos-getdents64.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0013-makos-working-directory.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	complete_series
fi

if grep -q 'M_poll = 91' "$source_dir/src/internal/makos_syscall.c"; then
	patch="$port_dir/patches/0011-makos-posix-stat.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0012-makos-getdents64.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0013-makos-working-directory.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	complete_series
fi

if grep -q 'M_fd_seek = 87' "$source_dir/src/internal/makos_syscall.c"; then
	patch="$port_dir/patches/0009-makos-fd-control.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0010-makos-pipe-poll.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0011-makos-posix-stat.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0012-makos-getdents64.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0013-makos-working-directory.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	complete_series
fi

if grep -q 'M_process_identity = 85' "$source_dir/src/internal/makos_syscall.c"; then
	patch="$port_dir/patches/0008-makos-fd-dup-seek.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0009-makos-fd-control.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0010-makos-pipe-poll.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0011-makos-posix-stat.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0012-makos-getdents64.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0013-makos-working-directory.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	complete_series
fi

if grep -q 'M_clock_realtime = 84' "$source_dir/src/internal/makos_syscall.c"; then
	patch="$port_dir/patches/0007-makos-process-identity.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0008-makos-fd-dup-seek.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0009-makos-fd-control.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0010-makos-pipe-poll.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0011-makos-posix-stat.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0012-makos-getdents64.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0013-makos-working-directory.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	complete_series
fi

if grep -q 'M_getrandom = 83' "$source_dir/src/internal/makos_syscall.c"; then
	for patch in \
		"$port_dir/patches/0006-makos-clock-realtime.patch" \
		"$port_dir/patches/0007-makos-process-identity.patch" \
		"$port_dir/patches/0008-makos-fd-dup-seek.patch" \
		"$port_dir/patches/0009-makos-fd-control.patch" \
		"$port_dir/patches/0010-makos-pipe-poll.patch" \
		"$port_dir/patches/0011-makos-posix-stat.patch" \
		"$port_dir/patches/0012-makos-getdents64.patch" \
		"$port_dir/patches/0013-makos-working-directory.patch"
	do
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	done
	complete_series
fi

if grep -q 'M_futex = 82' "$source_dir/src/internal/makos_syscall.c" \
	&& grep -Fq '(c && !(c & PROT_READ))' "$source_dir/src/internal/makos_syscall.c"; then
	for patch in \
		"$port_dir/patches/0005-makos-getrandom.patch" \
		"$port_dir/patches/0006-makos-clock-realtime.patch" \
		"$port_dir/patches/0007-makos-process-identity.patch" \
		"$port_dir/patches/0008-makos-fd-dup-seek.patch" \
		"$port_dir/patches/0009-makos-fd-control.patch" \
		"$port_dir/patches/0010-makos-pipe-poll.patch" \
		"$port_dir/patches/0011-makos-posix-stat.patch" \
		"$port_dir/patches/0012-makos-getdents64.patch" \
		"$port_dir/patches/0013-makos-working-directory.patch"
	do
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	done
	complete_series
fi

if grep -q 'M_futex = 82' "$source_dir/src/internal/makos_syscall.c" \
	&& grep -q 'SYS_thread_clone' "$source_dir/src/thread/aarch64/clone.s"; then
	for patch in \
		"$port_dir/patches/0004-makos-prot-none-guards.patch" \
		"$port_dir/patches/0005-makos-getrandom.patch" \
		"$port_dir/patches/0006-makos-clock-realtime.patch" \
		"$port_dir/patches/0007-makos-process-identity.patch" \
		"$port_dir/patches/0008-makos-fd-dup-seek.patch" \
		"$port_dir/patches/0009-makos-fd-control.patch" \
		"$port_dir/patches/0010-makos-pipe-poll.patch" \
		"$port_dir/patches/0011-makos-posix-stat.patch" \
		"$port_dir/patches/0012-makos-getdents64.patch" \
		"$port_dir/patches/0013-makos-working-directory.patch"
	do
		git -C "$source_dir" apply --check "$patch"
		git -C "$source_dir" apply "$patch"
	done
	complete_series
fi

if grep -q 'M_set_tid_address = 78' "$source_dir/src/internal/makos_syscall.c" \
	&& grep -q 'case L_gettid:' "$source_dir/src/internal/makos_syscall.c"; then
	patch="$port_dir/patches/0003-makos-pthread-clone-futex.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0004-makos-prot-none-guards.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0005-makos-getrandom.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0006-makos-clock-realtime.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0007-makos-process-identity.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0008-makos-fd-dup-seek.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0009-makos-fd-control.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0010-makos-pipe-poll.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0011-makos-posix-stat.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0012-makos-getdents64.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	patch="$port_dir/patches/0013-makos-working-directory.patch"
	git -C "$source_dir" apply --check "$patch"
	git -C "$source_dir" apply "$patch"
	complete_series
fi

fi

for patch in "$port_dir"/patches/*.patch; do
	if git -C "$source_dir" apply --check "$patch" 2>/dev/null; then
		git -C "$source_dir" apply "$patch"
	elif git -C "$source_dir" apply --reverse --check "$patch" 2>/dev/null; then
		:
	else
		echo "musl patch state invalid: $patch" >&2
		exit 1
	fi
done

apply_final_fixes
git -C "$source_dir" diff --check
echo "MAKOS_MUSL_PATCHES_OK revision=$MUSL_COMMIT patches=64"
