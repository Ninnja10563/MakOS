/* SPDX-License-Identifier: MIT */
#define _GNU_SOURCE
#include <dirent.h>
#include <pthread.h>
#include <errno.h>
#include <fcntl.h>
#include <netdb.h>
#include <poll.h>
#include <sched.h>
#include <signal.h>
#include <stdio.h>
#include <stdint.h>
#include <netinet/in.h>
#include <sys/epoll.h>
#include <sys/mman.h>
#include <sys/random.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>
#include <string.h>

static volatile unsigned shared_value;
static _Thread_local unsigned tls_value;
static long main_tid;
static long worker_tid;
static pid_t main_parent_pid;
static volatile unsigned worker_ready;
static volatile unsigned event_phase;
static volatile unsigned exit_group_worker_ready;
static volatile unsigned robust_worker_ready;
static volatile unsigned robust_waiting;
static uint64_t event_handle;
static volatile sig_atomic_t winch_count;
static volatile long winch_tid;
static volatile long target_signal_tid;
static volatile unsigned target_signal_ready;
static volatile sig_atomic_t target_signal_goal;
static pthread_mutex_t timed_mutex;
static volatile unsigned timed_mutex_ready;
static volatile unsigned timed_mutex_release;
static pthread_mutex_t requeue_mutex;
static pthread_cond_t requeue_condition;
static unsigned requeue_ready;
static unsigned requeue_release;
static unsigned requeue_completed;
static volatile unsigned production_smp_ready;
static volatile unsigned production_smp_release;
static volatile unsigned production_affinity_failed;
static volatile unsigned production_auto_waiting;
static volatile unsigned production_auto_release;
static volatile unsigned production_input_ready;
static long production_input_surface;
static long production_input_decoy_surface;

struct makos_surface_event {
	uint32_t kind;
	uint32_t key;
	uint32_t modifiers;
	int32_t x;
	int32_t y;
	uint32_t width;
	uint32_t height;
};

_Static_assert(sizeof(struct makos_surface_event) == 28,
	"MakOS surface event ABI size");

struct robust_probe_head {
	volatile void *volatile head;
	long offset;
	volatile void *volatile pending;
};

struct robust_probe_node {
	volatile void *volatile next;
	volatile unsigned futex;
};

static struct robust_probe_head robust_head;
static struct robust_probe_node robust_node;

static long makos_call(long number, long first, long second);
static long makos_call4(long number, long first, long second, long third,
	long fourth);

static void winch_handler(int signal)
{
	if (signal == SIGWINCH) {
		winch_tid = syscall(SYS_gettid);
		winch_count++;
	}
}

static void *production_smp_worker(void *argument)
{
	unsigned index = (unsigned)(uintptr_t)argument;
	unsigned first_cpu = index + 1;
	unsigned second_cpu = (index + 1) % 3 + 1;
	cpu_set_t requested;
	cpu_set_t observed;
	/*
	 * Keep the default 0xe affinity while creating a genuine dispatch-load
	 * imbalance. Workers 1 and 2 sleep in the kernel; worker 0 repeatedly
	 * yields without choosing a CPU. The kernel must place all three workers
	 * and move worker 0 based on scheduler load before any affinity syscall.
	 */
	if (index == 0) {
		while (__atomic_load_n(&production_auto_waiting, __ATOMIC_ACQUIRE) != 0x6)
			makos_call(1, 0, 0);
		for (unsigned iteration = 0; iteration < 4096; iteration++)
			makos_call(1, 0, 0);
		__atomic_store_n(&production_auto_release, 1, __ATOMIC_RELEASE);
	} else {
		const struct timespec pause = { .tv_sec = 0, .tv_nsec = 1000000 };
		__atomic_fetch_or(&production_auto_waiting, 1U << index, __ATOMIC_RELEASE);
		while (!__atomic_load_n(&production_auto_release, __ATOMIC_ACQUIRE))
			nanosleep(&pause, 0);
	}
	CPU_ZERO(&requested);
	CPU_SET(first_cpu, &requested);
	if (sched_setaffinity(0, sizeof requested, &requested) ||
	    sched_getaffinity(0, sizeof observed, &observed) ||
	    !CPU_ISSET(first_cpu, &observed) || CPU_COUNT(&observed) != 1)
		__atomic_fetch_or(&production_affinity_failed, 1U << index,
			__ATOMIC_RELEASE);
	CPU_ZERO(&requested);
	CPU_SET(second_cpu, &requested);
	if (sched_setaffinity(0, sizeof requested, &requested) ||
	    sched_getaffinity(0, sizeof observed, &observed) ||
	    !CPU_ISSET(second_cpu, &observed) || CPU_COUNT(&observed) != 1)
		__atomic_fetch_or(&production_affinity_failed, 1U << index,
			__ATOMIC_RELEASE);
	/* Restore the production AP pool after proving a forced migration. */
	CPU_ZERO(&requested);
	CPU_SET(1, &requested);
	CPU_SET(2, &requested);
	CPU_SET(3, &requested);
	if (sched_setaffinity(0, sizeof requested, &requested) ||
	    sched_getaffinity(0, sizeof observed, &observed) ||
	    !CPU_ISSET(1, &observed) || !CPU_ISSET(2, &observed) ||
	    !CPU_ISSET(3, &observed) || CPU_COUNT(&observed) != 3)
		__atomic_fetch_or(&production_affinity_failed, 1U << index,
			__ATOMIC_RELEASE);
	__atomic_fetch_or(&production_smp_ready, 1U << index, __ATOMIC_RELEASE);
	while (!__atomic_load_n(&production_smp_release, __ATOMIC_ACQUIRE))
		makos_call(1, 0, 0);
	return 0;
}

static void *production_input_watcher(void *argument)
{
	struct makos_surface_event event = {0};
	long surface = (long)(uintptr_t)argument;
	unsigned ready = surface == production_input_surface ? 1U : 2U;
	__atomic_fetch_or(&production_input_ready, ready, __ATOMIC_RELEASE);
	long result = makos_call4(140, surface, (long)&event,
		sizeof event, 0);
	if (surface == production_input_decoy_surface)
		return result == -1 ? 0 : (void *)(uintptr_t)2;
	if (result != sizeof event || event.kind != 1 || event.key != 132)
		return (void *)(uintptr_t)1;
	return 0;
}

static int production_smp_worker_probe(void)
{
	pthread_t workers[3];
	void *result = 0;
	production_smp_ready = 0;
	production_smp_release = 0;
	production_affinity_failed = 0;
	production_auto_waiting = 0;
	production_auto_release = 0;
	cpu_set_t leader_affinity;
	if (sched_getaffinity(0, sizeof leader_affinity, &leader_affinity) ||
	    !CPU_ISSET(0, &leader_affinity) || CPU_COUNT(&leader_affinity) != 1)
		return 10;
	for (unsigned index = 0; index < 3; index++)
		if (pthread_create(&workers[index], 0, production_smp_worker,
		    (void *)(uintptr_t)index))
			return 1;
	for (unsigned attempts = 0; attempts < 100000; attempts++) {
		if (__atomic_load_n(&production_smp_ready, __ATOMIC_ACQUIRE) == 0x7)
			break;
		makos_call(1, 0, 0);
	}
	if (__atomic_load_n(&production_smp_ready, __ATOMIC_ACQUIRE) != 0x7)
		return 2;
	if (__atomic_load_n(&production_affinity_failed, __ATOMIC_ACQUIRE))
		return 11;
	/* Keep every worker Ready until the kernel observes concurrent AP owners. */
	for (unsigned attempts = 0; attempts < 1000; attempts++) makos_call(1, 0, 0);
	__atomic_store_n(&production_smp_release, 1, __ATOMIC_RELEASE);
	for (unsigned index = 0; index < 3; index++)
		if (pthread_join(workers[index], &result) || result) return 3;
	return 0;
}

static int production_smp_overlap_probe(void)
{
	pthread_t input_watcher;
	void *result = 0;
	int worker_result = production_smp_worker_probe();
	if (worker_result) return worker_result;
	production_input_decoy_surface = makos_call4(8, 96, 64, 8, 0);
	if (production_input_decoy_surface <= 0 ||
	    makos_call4(10, production_input_decoy_surface, 0, 0, 0) != 1)
		return 4;
	production_input_surface = makos_call4(8, 96, 64, 7, 0);
	if (production_input_surface <= 0 ||
	    makos_call4(10, production_input_surface, 0, 0, 0) != 1)
		return 4;
	production_input_ready = 0;
	pthread_t decoy_watcher;
	if (pthread_create(&input_watcher, 0, production_input_watcher,
	    (void *)(uintptr_t)production_input_surface) ||
	    pthread_create(&decoy_watcher, 0, production_input_watcher,
	    (void *)(uintptr_t)production_input_decoy_surface))
		return 5;
	for (unsigned attempts = 0; attempts < 100000; attempts++) {
		if (__atomic_load_n(&production_input_ready, __ATOMIC_ACQUIRE) == 3) break;
		makos_call(1, 0, 0);
	}
	if (__atomic_load_n(&production_input_ready, __ATOMIC_ACQUIRE) != 3) return 6;
	if (pthread_join(input_watcher, &result) || result) return 7;
	if (makos_call4(123, production_input_decoy_surface, 0, 0, 0) != 1 ||
	    pthread_join(decoy_watcher, &result) || result)
		return 12;
	if (makos_call4(123, production_input_surface, 0, 0, 0) != 1) return 8;
	static const char marker[] =
		"MAKOS_FIREFOX_SMP_PTHREAD_OVERLAP_OK workers=3 rendezvous=ready release=bounded affinity=default:0xe,explicit singleton=0x2,0x4,0x8 restored=0xe get=kernel-owned placement=least-reserved-ap migrations=automatic:load,forced:3 caller_selected_automatic=0\n"
		"MAKOS_FIREFOX_SMP_INPUT_PRIORITY_OK key=132 watcher=nonleader dispatch=ap leader=cpu0 wait=surface-event routing=exact-handle decoy=blocked-until-destroy\n";
	if (write(1, marker, sizeof marker - 1) != sizeof marker - 1) return 9;
	return 0;
}

static int native_smp_overlap_probe(void)
{
	int worker_result = production_smp_worker_probe();
	if (worker_result) return worker_result;
	static const char marker[] =
		"MAKOS_NATIVE_SMP_PTHREAD_OVERLAP_OK workers=3 rendezvous=ready "
		"release=bounded affinity=default:0xe,explicit singleton=0x2,0x4,0x8 "
		"restored=0xe get=kernel-owned placement=least-reserved-ap "
		"migrations=automatic:load,forced:3 caller_selected_automatic=0\n";
	if (write(1, marker, sizeof marker - 1) != sizeof marker - 1) return 9;
	return 0;
}

struct pipe_worker_args {
	int read_fd;
	int write_fd;
	int read_mode;
};

static long makos_call(long number, long first, long second)
{
	register long x0 __asm__("x0") = first;
	register long x1 __asm__("x1") = second;
	register long x8 __asm__("x8") = number;
	__asm__ __volatile__("svc 0" : "+r"(x0) : "r"(x1), "r"(x8) : "memory", "cc");
	return x0;
}

static long makos_call4(long number, long first, long second, long third,
	long fourth)
{
	register long x0 __asm__("x0") = first;
	register long x1 __asm__("x1") = second;
	register long x2 __asm__("x2") = third;
	register long x3 __asm__("x3") = fourth;
	register long x8 __asm__("x8") = number;
	__asm__ __volatile__("svc 0" : "+r"(x0)
		: "r"(x1), "r"(x2), "r"(x3), "r"(x8) : "memory", "cc");
	return x0;
}

struct makos_typed_message {
	uint8_t version;
	uint8_t length;
	uint16_t type;
	uint32_t sender_pid;
	uint32_t sender_uid;
	uint8_t payload[52];
};

_Static_assert(sizeof(struct makos_typed_message) == 64,
	"typed IPC wire size");

static long typed_wait_accept(long listener)
{
	for (unsigned attempts = 0; attempts < 10000; attempts++) {
		long result = makos_call(145, listener, 0);
		if (result != -1) return result;
		makos_call(1, 0, 0);
	}
	return -1;
}

static int typed_wait_receive(long endpoint, struct makos_typed_message *message,
	uint64_t *transfer)
{
	for (unsigned attempts = 0; attempts < 10000; attempts++) {
		if (!makos_call4(147, endpoint, (long)message, (long)transfer, 0))
			return 0;
		makos_call(1, 0, 0);
	}
	return -1;
}

static int typed_ipc_probe(void)
{
	static const char service[] = "org.makos.echo";
	static const char child_service[] = "org.makos.child";
	static const char unknown[] = "org.makos.missing";
	struct makos_typed_message message = {.version = 1, .length = 4, .type = 7};
	struct makos_typed_message received;
	uint64_t transfer = 0;
	long listener = makos_call(143, (long)service, sizeof service - 1);
	if (listener == -1 || makos_call(143, (long)service, sizeof service - 1) != -1 ||
	    makos_call(144, (long)unknown, sizeof unknown - 1) != -1)
		return 1;
	pid_t child = fork();
	if (child < 0) return 2;
	if (!child) {
		long child_listener = makos_call(143, (long)child_service,
			sizeof child_service - 1);
		long primary = makos_call(144, (long)service, sizeof service - 1);
		long delegated = makos_call(144, (long)service, sizeof service - 1);
		if (child_listener == -1 || primary == -1 || delegated == -1) _exit(3);
		memcpy(message.payload, "ping", 4);
		message.version = 0;
		if (makos_call4(146, primary, (long)&message, 0, 0) != -1) _exit(4);
		message.version = 1;
		if (makos_call4(146, primary, (long)&message, delegated, 0x80) != -1)
			_exit(5);
		if (makos_call4(146, primary, (long)&message, delegated, 1) ||
		    makos_call(35, delegated, 0) != 1)
			_exit(6);
		message.type = 17;
		memcpy(message.payload, "fifo", 4);
		if (makos_call4(146, primary, (long)&message, 0, 0)) _exit(9);
		if (typed_wait_receive(primary, &received, &transfer) || transfer ||
		    received.version != 1 || received.type != 8 || received.length != 4 ||
		    memcmp(received.payload, "pong", 4))
			_exit(7);
		/* Process exit must close primary and child_listener immediately. */
		_exit(0);
	}
	long primary = typed_wait_accept(listener);
	long delegated_server = typed_wait_accept(listener);
	if (primary == -1 || delegated_server == -1) return 9;
	memset(&received, 0, sizeof received);
	if (typed_wait_receive(primary, &received, &transfer) || !transfer ||
	    received.version != 1 || received.type != 7 || received.length != 4 ||
	    received.sender_pid != (uint32_t)child ||
	    received.sender_uid != (uint32_t)getuid() ||
	    memcmp(received.payload, "ping", 4))
		return 10;
	uint64_t delegated_transfer = transfer;
	transfer = UINT64_MAX;
	if (typed_wait_receive(primary, &received, &transfer) || transfer ||
	    received.version != 1 || received.type != 17 || received.length != 4 ||
	    received.sender_pid != (uint32_t)child ||
	    memcmp(received.payload, "fifo", 4))
		return 17;
	uint64_t ignored = UINT64_MAX;
	if (makos_call4(147, delegated_transfer, (long)&received,
	    (long)&ignored, 0) != -1)
		return 11;
	message.type = 9;
	memcpy(message.payload, "life", 4);
	if (makos_call4(146, delegated_transfer, (long)&message, 0, 0) ||
	    typed_wait_receive(delegated_server, &received, &ignored) ||
	    ignored || received.type != 9 || memcmp(received.payload, "life", 4))
		return 12;
	if (makos_call(35, delegated_transfer, 0) != 1 ||
	    makos_call4(146, delegated_transfer, (long)&message, 0, 0) != -1)
		return 13;
	long cleanup_client = makos_call(144, (long)child_service,
		sizeof child_service - 1);
	if (cleanup_client == -1) return 18;
	message.type = 8;
	memcpy(message.payload, "pong", 4);
	if (makos_call4(146, primary, (long)&message, 0, 0)) return 14;
	long child_status = 0;
	for (unsigned attempts = 0; attempts < 10000 && !child_status; attempts++) {
		child_status = makos_call(126, child, 0);
		if (!child_status) makos_call(1, 0, 0);
	}
	if (child_status != 1 ||
	    makos_call4(146, primary, (long)&message, 0, 0) != -1 ||
	    makos_call4(146, cleanup_client, (long)&message, 0, 0) != -1 ||
	    makos_call(144, (long)child_service, sizeof child_service - 1) != -1)
		return 19;
	siginfo_t child_info;
	memset(&child_info, 0, sizeof child_info);
	if (waitid(P_PID, child, &child_info, WEXITED) ||
	    child_info.si_pid != child || child_info.si_code != CLD_EXITED ||
	    child_info.si_status)
		return 15;
	if (makos_call(35, primary, 0) != 1 ||
	    makos_call(35, delegated_server, 0) != 1 ||
	    makos_call(35, cleanup_client, 0) != 1 ||
	    makos_call(35, listener, 0) != 1 ||
	    makos_call(144, (long)service, sizeof service - 1) != -1)
		return 16;
	return 0;
}

static void *exit_group_worker(void *argument)
{
	(void)argument;
	exit_group_worker_ready = 1;
	for (;;) makos_call(1, 0, 0);
}

static void *robust_worker(void *argument)
{
	long tid;
	(void)argument;
	robust_head.head = &robust_node.next;
	robust_head.offset = (char *)&robust_node.futex - (char *)&robust_node.next;
	robust_head.pending = 0;
	robust_node.next = &robust_head.head;
	if (syscall(SYS_set_robust_list, &robust_head, sizeof robust_head))
		return (void *)1;
	tid = syscall(SYS_gettid);
	if (tid <= 0 || tid > 0x3fffffff) return (void *)2;
	robust_node.futex = (unsigned)tid | 0x80000000U;
	robust_worker_ready = 1;
	while (!robust_waiting) makos_call(1, 0, 0);
	makos_call(1, 0, 0);
	makos_call(81, 0, 0);
	return (void *)3;
}

static void *worker(void *argument)
{
	if ((uintptr_t)argument != 73 || tls_value != 0) return (void *)1;
	if (getppid() != main_parent_pid) return (void *)3;
	worker_tid = syscall(SYS_gettid);
	tls_value = 22;
	shared_value = 73;
	worker_ready = 1;
	while (!event_phase) makos_call(1, 0, 0);
	/* Yield back after parent arms event wait; next dispatch signals it. */
	makos_call(1, 0, 0);
	makos_call(1, 0, 0);
	if (makos_call(33, event_handle, 0) != 1) return (void *)2;
	return (void *)(uintptr_t)0x1234;
}

static void *pipe_worker(void *argument)
{
	struct pipe_worker_args *args = argument;
	unsigned char bytes[512];
	for (unsigned index = 0; index < 4; index++) makos_call(1, 0, 0);
	if (args->read_mode)
		return read(args->read_fd, bytes, sizeof bytes) == sizeof bytes
			? 0 : (void *)1;
	return write(args->write_fd, "B", 1) == 1 ? 0 : (void *)2;
}

static void *mask_worker(void *argument)
{
	sigset_t observed;
	(void)argument;
	if (pthread_sigmask(SIG_SETMASK, 0, &observed)) return (void *)1;
	return sigismember(&observed, SIGWINCH) == 1 ? 0 : (void *)2;
}

static void *target_signal_worker(void *argument)
{
	(void)argument;
	target_signal_tid = syscall(SYS_gettid);
	target_signal_ready = 1;
	while (winch_count < target_signal_goal) makos_call(1, 0, 0);
	return winch_tid == target_signal_tid ? 0 : (void *)1;
}

static void *timed_mutex_worker(void *argument)
{
	(void)argument;
	if (pthread_mutex_lock(&timed_mutex)) return (void *)1;
	timed_mutex_ready = 1;
	while (!timed_mutex_release) makos_call(1, 0, 0);
	return pthread_mutex_unlock(&timed_mutex) ? (void *)2 : 0;
}

static void *requeue_worker(void *argument)
{
	(void)argument;
	if (pthread_mutex_lock(&requeue_mutex)) return (void *)1;
	requeue_ready++;
	while (!requeue_release)
		if (pthread_cond_wait(&requeue_condition, &requeue_mutex))
			return (void *)2;
	requeue_completed++;
	return pthread_mutex_unlock(&requeue_mutex) ? (void *)3 : 0;
}

static int dns_a_record(const unsigned char *packet, size_t length,
	unsigned char output[4])
{
	for (size_t index = 12; index + 16 <= length; index++) {
		if ((packet[index] & 0xc0) == 0xc0 &&
		    packet[index + 2] == 0 && packet[index + 3] == 1 &&
		    packet[index + 4] == 0 && packet[index + 5] == 1 &&
		    packet[index + 10] == 0 && packet[index + 11] == 4) {
			memcpy(output, packet + index + 12, 4);
			return 1;
		}
	}
	return 0;
}

static int scalable_directory_probe(int remount_phase)
{
	char path[sizeof "scale/" + 255];
	char long_name[256];
	unsigned char seen[64] = {0};
	unsigned seen_long = 0;
	DIR *directory;
	struct dirent *entry;
	memset(long_name, 'l', sizeof long_name - 1);
	long_name[sizeof long_name - 1] = 0;

	if (!remount_phase) {
		if (mkdir("scale", 0700)) return 1;
		for (unsigned index = 0; index < sizeof seen; index++) {
			memcpy(path, "scale/entry-", 12);
			path[12] = (char)('0' + index / 100u);
			path[13] = (char)('0' + index / 10u % 10u);
			path[14] = (char)('0' + index % 10u);
			path[15] = 0;
			int fd = open(path, O_CREAT | O_EXCL | O_WRONLY, 0600);
			if (fd < 0 || close(fd)) return 3;
		}
		memcpy(path, "scale/", 6);
		memcpy(path + 6, long_name, sizeof long_name);
		int fd = open(path, O_CREAT | O_EXCL | O_WRONLY, 0600);
		if (fd < 0 || write(fd, "L", 1) != 1 || fsync(fd) || close(fd)) return 5;
	}

	directory = opendir("scale");
	if (!directory) return 6;
	errno = 0;
	while ((entry = readdir(directory))) {
		unsigned index;
		if (!strcmp(entry->d_name, ".") || !strcmp(entry->d_name, ".."))
			continue;
		if (!strcmp(entry->d_name, long_name)) {
			if (entry->d_type != DT_REG || seen_long) return 7;
			seen_long = 1;
			continue;
		}
		if (strlen(entry->d_name) != 9 ||
		    memcmp(entry->d_name, "entry-", 6) ||
		    entry->d_name[6] < '0' || entry->d_name[6] > '9' ||
		    entry->d_name[7] < '0' || entry->d_name[7] > '9' ||
		    entry->d_name[8] < '0' || entry->d_name[8] > '9')
			return 8;
		index = (unsigned)(entry->d_name[6] - '0') * 100u +
			(unsigned)(entry->d_name[7] - '0') * 10u +
			(unsigned)(entry->d_name[8] - '0');
		if (index >= sizeof seen || entry->d_type != DT_REG || seen[index])
			return 8;
		seen[index] = 1;
	}
	if (errno || closedir(directory) || !seen_long) return 9;
	for (unsigned index = 0; index < sizeof seen; index++)
		if (!seen[index]) return 10;

	memcpy(path, "scale/", 6);
	memcpy(path + 6, long_name, sizeof long_name);
	int fd = open(path, O_RDONLY);
	char value = 0;
	if (fd < 0 || read(fd, &value, 1) != 1 || value != 'L' || close(fd)) return 12;
	if (!remount_phase) return 0;

	for (unsigned index = 0; index < sizeof seen; index++) {
		memcpy(path, "scale/entry-", 12);
		path[12] = (char)('0' + index / 100u);
		path[13] = (char)('0' + index / 10u % 10u);
		path[14] = (char)('0' + index % 10u);
		path[15] = 0;
		if (unlink(path)) return 13;
	}
	memcpy(path, "scale/", 6);
	memcpy(path + 6, long_name, sizeof long_name);
	if (unlink(path) || rmdir("scale"))
		return 14;
	return 0;
}

static int scm_rights_probe(void)
{
	int pair[2] = {-1, -1};
	int source = -1;
	int received = -1;
	char byte = 0;
	char content[6] = {0};
	union {
		struct cmsghdr alignment;
		unsigned char bytes[CMSG_SPACE(sizeof(int))];
	} control = {0};
	struct iovec vector = {.iov_base = &byte, .iov_len = 1};
	struct msghdr message = {
		.msg_iov = &vector,
		.msg_iovlen = 1,
		.msg_control = control.bytes,
		.msg_controllen = sizeof control.bytes,
	};

	if (socketpair(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0, pair)) return 1;
	source = open("rights-source.txt", O_CREAT | O_EXCL | O_RDWR, 0600);
	if (source < 0 || write(source, "rights", 6) != 6 ||
	    lseek(source, 0, SEEK_SET) != 0 || write(pair[0], "A", 1) != 1)
		return 2;
	byte = 'B';
	struct cmsghdr *header = CMSG_FIRSTHDR(&message);
	if (!header) return 3;
	header->cmsg_level = SOL_SOCKET;
	header->cmsg_type = SCM_RIGHTS;
	header->cmsg_len = CMSG_LEN(sizeof source);
	memcpy(CMSG_DATA(header), &source, sizeof source);
	if (sendmsg(pair[0], &message, 0) != 1 || close(source)) return 4;
	source = -1;

	memset(control.bytes, 0, sizeof control.bytes);
	byte = 0;
	message.msg_controllen = sizeof control.bytes;
	if (recvmsg(pair[1], &message, 0) != 1 || byte != 'A' ||
	    message.msg_controllen != 0)
		return 5;
	memset(control.bytes, 0, sizeof control.bytes);
	byte = 0;
	message.msg_controllen = sizeof control.bytes;
	if (recvmsg(pair[1], &message, 0) != 1 || byte != 'B') return 6;
	header = CMSG_FIRSTHDR(&message);
	if (!header || header->cmsg_level != SOL_SOCKET ||
	    header->cmsg_type != SCM_RIGHTS ||
	    header->cmsg_len != CMSG_LEN(sizeof received))
		return 7;
	memcpy(&received, CMSG_DATA(header), sizeof received);
	if (received < 0 || read(received, content, sizeof content) != sizeof content ||
	    memcmp(content, "rights", sizeof content))
		return 8;
	if (close(received) || close(pair[0]) || close(pair[1]) ||
	    unlink("rights-source.txt"))
		return 9;
	return 0;
}

int main(int argc, char **argv)
{
	pthread_t thread;
	void *result = 0;
	int pipe_fds[2] = {-1, -1};
	int blocking_pipe[2] = {-1, -1};
	int epoll_fd = -1;
	int socket_fd = -1;
	int note_fd = -1;
	int persistent_phase = 0;
	struct pollfd readiness[2];
	struct epoll_event epoll_events[2];
	fd_set select_read;
	fd_set select_write;
	struct stat path_metadata;
	struct stat fd_metadata;
	struct stat pipe_metadata;
	struct stat tty_metadata;
	struct stat directory_metadata;
	struct stat link_metadata;
	struct stat target_metadata;
	DIR *directory;
	struct dirent *directory_entry;
	unsigned saw_dot = 0;
	unsigned saw_dotdot = 0;
	unsigned saw_note = 0;
	unsigned saw_link = 0;
	char cwd[64];
	char link_output[64];
	unsigned char pipe_input[512];
	unsigned char pipe_output[512];
	unsigned char file_output[16];
	unsigned char resolved_ip[4];
	uint64_t channels[2] = {0, 0};
	unsigned char random_a[32];
	unsigned char random_b[32];
	unsigned random_or = 0;
	unsigned random_diff = 0;
	struct timespec realtime;
	struct timespec resolution;
	struct timespec sleep_before;
	struct timespec sleep_after;
	struct timespec sleep_request;
	struct sigaction winch_action = {0};
	sigset_t winch_set;
	sigset_t empty_set;
	sigset_t observed_set;
	if (argc == 2 && !strcmp(argv[1], "production-smp") &&
	    production_smp_overlap_probe())
		return 255;
	if (argc == 2 && !strcmp(argv[1], "native-smp") &&
	    native_smp_overlap_probe())
		return 253;
	main_parent_pid = getppid();
	if (main_parent_pid <= 0 || main_parent_pid == getpid()) return 221;
	main_tid = syscall(SYS_gettid);
	tls_value = 11;
	event_handle = makos_call(32, 0, 0);
	if (event_handle == UINT64_MAX) return 109;
	if (pthread_create(&thread, 0, worker, (void *)(uintptr_t)73)) return 110;
	while (!worker_ready) makos_call(1, 0, 0);
	event_phase = 1;
	if (makos_call(34, event_handle, 0) != 1) return 114;
	if (pthread_join(thread, &result)) return 111;
	if (shared_value != 73 || tls_value != 11 || worker_tid == main_tid ||
	    result != (void *)(uintptr_t)0x1234)
		return 112;
	if (makos_call(2, (long)channels, 0) != 0) return 115;
	if (makos_call(3, channels[0], 0x4950434f4b) != 0 ||
	    makos_call(4, channels[1], 0) != 0x4950434f4b)
		return 116;
	if (makos_call(3, channels[1], 0x4241434b) != 0 ||
	    makos_call(4, channels[0], 0) != 0x4241434b)
		return 117;
	if (makos_call(35, channels[0], 0) != 1 ||
	    makos_call(35, channels[1], 0) != 1 ||
	    makos_call(35, event_handle, 0) != 1)
		return 118;
	int typed_ipc_result = typed_ipc_probe();
	if (typed_ipc_result) {
		char failed[] = "MAKOS_MUSL_TYPED_IPC_FAIL code=00\n";
		failed[31] = '0' + (typed_ipc_result / 10) % 10;
		failed[32] = '0' + typed_ipc_result % 10;
		write(1, failed, sizeof failed - 1);
		return 183;
	}
	static const char typed_ipc_marker[] =
		"MAKOS_MUSL_TYPED_IPC_OK service=org.makos.echo client=child "
		"typed=1 fifo=1 transfer=channel rights=attenuated spoof_denied=1 "
		"stale_denied=1 cleanup=1\n";
	if (write(1, typed_ipc_marker, sizeof typed_ipc_marker - 1) !=
	    sizeof typed_ipc_marker - 1)
		return 183;
	if (scm_rights_probe()) return 254;
	static const char scm_rights_marker[] =
		"MAKOS_MUSL_SCM_RIGHTS_OK socketpair=unix-stream "
		"ordering=associated-byte lifetime=queued-open-description payload=read-after-sender-close\n";
	if (write(1, scm_rights_marker, sizeof scm_rights_marker - 1) !=
	    sizeof scm_rights_marker - 1)
		return 254;
	if (getrandom(random_a, sizeof random_a, 0) != sizeof random_a ||
	    getrandom(random_b, sizeof random_b, GRND_NONBLOCK) != sizeof random_b)
		return 119;
	for (unsigned index = 0; index < sizeof random_a; index++) {
		random_or |= random_a[index] | random_b[index];
		random_diff |= random_a[index] ^ random_b[index];
		random_a[index] = 0;
		random_b[index] = 0;
	}
	if (!random_or || !random_diff) return 120;
	if (clock_gettime(CLOCK_REALTIME, &realtime) ||
	    realtime.tv_sec < 1577836800 || realtime.tv_nsec != 0)
		return 121;
	if (clock_getres(CLOCK_MONOTONIC, &resolution) || resolution.tv_sec != 0 ||
	    resolution.tv_nsec != 10000000 ||
	    clock_getres(CLOCK_REALTIME, &resolution) || resolution.tv_sec != 1 ||
	    resolution.tv_nsec != 0)
		return 185;
	sleep_request = (struct timespec){.tv_sec = 0, .tv_nsec = 30000000};
	if (clock_gettime(CLOCK_MONOTONIC, &sleep_before) ||
	    nanosleep(&sleep_request, 0) ||
	    clock_gettime(CLOCK_MONOTONIC, &sleep_after))
		return 186;
	long long elapsed_ns =
		(sleep_after.tv_sec - sleep_before.tv_sec) * 1000000000LL +
		(sleep_after.tv_nsec - sleep_before.tv_nsec);
	if (elapsed_ns < 30000000LL || elapsed_ns > 1000000000LL) return 187;
	sleep_request = sleep_after;
	sleep_request.tv_nsec += 20000000;
	if (sleep_request.tv_nsec >= 1000000000) {
		sleep_request.tv_sec++;
		sleep_request.tv_nsec -= 1000000000;
	}
	if (clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &sleep_request, 0) ||
	    clock_gettime(CLOCK_MONOTONIC, &sleep_after) ||
	    sleep_after.tv_sec < sleep_request.tv_sec ||
	    (sleep_after.tv_sec == sleep_request.tv_sec &&
	     sleep_after.tv_nsec < sleep_request.tv_nsec))
		return 188;
	sleep_request = (struct timespec){.tv_sec = 0, .tv_nsec = 10000000};
	if (clock_nanosleep(CLOCK_REALTIME, 0, &sleep_request, 0)) return 189;
	sleep_request.tv_nsec = 1000000000;
	errno = 0;
	if (nanosleep(&sleep_request, 0) != -1 || errno != EINVAL) return 190;
	if (getpid() <= 0 || getuid() != geteuid() || getgid() != getegid())
		return 122;
	if (pipe2(pipe_fds, O_NONBLOCK | O_CLOEXEC)) return 123;
	if (fstat(pipe_fds[0], &pipe_metadata) || !S_ISFIFO(pipe_metadata.st_mode) ||
	    pipe_metadata.st_size != 0)
		return 135;
	if (fstat(1, &tty_metadata) || !S_ISCHR(tty_metadata.st_mode)) return 136;
	note_fd = open("/home/user/note.txt", O_RDONLY);
	if (note_fd < 0 || stat("/home/user/note.txt", &path_metadata) ||
	    fstat(note_fd, &fd_metadata))
		return 137;
	if (!S_ISREG(path_metadata.st_mode) || !S_ISREG(fd_metadata.st_mode) ||
	    path_metadata.st_ino != fd_metadata.st_ino ||
	    path_metadata.st_size != fd_metadata.st_size ||
	    path_metadata.st_uid != getuid() || close(note_fd))
		return 138;
	static const char metadata_target[] = "metadata-target.txt";
	static const char metadata_link[] = "metadata-link";
	note_fd = open(metadata_target, O_CREAT | O_WRONLY | O_TRUNC, 0600);
	if (note_fd < 0 || write(note_fd, "timestamp", 9) != 9 || fsync(note_fd) ||
	    fstat(note_fd, &target_metadata) || close(note_fd))
		return 231;
	if (!S_ISREG(target_metadata.st_mode) || target_metadata.st_size != 9 ||
	    target_metadata.st_atim.tv_sec < 1577836800 ||
	    target_metadata.st_mtim.tv_sec < 1577836800 ||
	    target_metadata.st_ctim.tv_sec < 1577836800 ||
	    target_metadata.st_atim.tv_nsec >= 1000000000 ||
	    target_metadata.st_mtim.tv_nsec >= 1000000000 ||
	    target_metadata.st_ctim.tv_nsec >= 1000000000)
		return 232;
	if (symlink(metadata_target, metadata_link)) return 233;
	ssize_t link_length = readlink(metadata_link, link_output, sizeof link_output);
	if (link_length != (ssize_t)(sizeof metadata_target - 1) ||
	    memcmp(link_output, metadata_target, sizeof metadata_target - 1))
		return 234;
	if (lstat(metadata_link, &link_metadata) ||
	    !S_ISLNK(link_metadata.st_mode) || link_metadata.st_size != link_length ||
	    stat(metadata_link, &path_metadata) || !S_ISREG(path_metadata.st_mode) ||
	    path_metadata.st_ino != target_metadata.st_ino ||
	    link_metadata.st_ino == target_metadata.st_ino)
		return 235;
	note_fd = open(metadata_link, O_RDONLY);
	memset(file_output, 0, sizeof file_output);
	if (note_fd < 0 || read(note_fd, file_output, 9) != 9 ||
	    memcmp(file_output, "timestamp", 9) || close(note_fd))
		return 236;
	directory = opendir("/home/user");
	if (!directory || fstat(dirfd(directory), &directory_metadata) ||
	    !S_ISDIR(directory_metadata.st_mode))
		return 139;
	errno = 0;
	while ((directory_entry = readdir(directory))) {
		if (!strcmp(directory_entry->d_name, ".")) saw_dot = 1;
		if (!strcmp(directory_entry->d_name, "..")) saw_dotdot = 1;
		if (!strcmp(directory_entry->d_name, "note.txt") &&
		    directory_entry->d_type == DT_REG)
			saw_note = 1;
		if (!strcmp(directory_entry->d_name, metadata_link) &&
		    directory_entry->d_type == DT_LNK)
			saw_link = 1;
	}
	if (errno || !saw_dot || !saw_dotdot || !saw_note || !saw_link) return 140;
	rewinddir(directory);
	directory_entry = readdir(directory);
	if (!directory_entry || strcmp(directory_entry->d_name, ".") ||
	    closedir(directory))
		return 141;
	errno = 0;
	if (unlink(metadata_link) || unlink(metadata_target) ||
	    !stat(metadata_link, &path_metadata) || errno != ENOENT)
		return 237;
	if (!getcwd(cwd, sizeof cwd) || strcmp(cwd, "/home/user")) return 142;
	if (chdir("..") || !getcwd(cwd, sizeof cwd) || strcmp(cwd, "/home"))
		return 143;
	note_fd = open("user/note.txt", O_RDONLY);
	if (note_fd < 0 || fstat(note_fd, &fd_metadata) ||
	    !S_ISREG(fd_metadata.st_mode) || close(note_fd))
		return 144;
	if (chdir("user/./../user") || !getcwd(cwd, sizeof cwd) ||
	    strcmp(cwd, "/home/user"))
		return 145;
	note_fd = open("cwd-test.txt", O_CREAT | O_WRONLY | O_TRUNC, 0600);
	if (note_fd < 0) return 146;
	if (write(note_fd, "cwd", 3) != 3) return 147;
	if (close(note_fd)) return 148;
	if (rename("cwd-test.txt", "cwd-renamed.txt")) return 149;
	if (stat("cwd-renamed.txt", &path_metadata)) return 150;
	if (path_metadata.st_size != 3) return 151;
	if (unlink("cwd-renamed.txt")) return 152;
	note_fd = open("rename-source.txt", O_CREAT | O_WRONLY | O_TRUNC, 0600);
	if (note_fd < 0 || write(note_fd, "new", 3) != 3 || close(note_fd)) return 153;
	note_fd = open("rename-destination.txt", O_CREAT | O_WRONLY | O_TRUNC, 0600);
	if (note_fd < 0 || write(note_fd, "old", 3) != 3 || close(note_fd) ||
	    rename("rename-source.txt", "rename-destination.txt")) return 153;
	note_fd = open("rename-destination.txt", O_RDONLY);
	memset(file_output, 0, sizeof file_output);
	if (note_fd < 0 || read(note_fd, file_output, 3) != 3 || close(note_fd) ||
	    memcmp(file_output, "new", 3) || unlink("rename-destination.txt"))
		return 153;
	note_fd = open("truncate-test.txt", O_CREAT | O_WRONLY | O_TRUNC, 0600);
	if (note_fd < 0) return 154;
	if (write(note_fd, "abcdef", 6) != 6 ||
	    lseek(note_fd, 0, SEEK_CUR) != 6 || ftruncate(note_fd, 10) ||
	    lseek(note_fd, 0, SEEK_CUR) != 6 || fsync(note_fd))
		return 155;
	int read_fd = open("truncate-test.txt", O_RDONLY);
	if (read_fd < 0 || read(read_fd, file_output, sizeof file_output) != 10 ||
	    close(read_fd))
		return 156;
	if (memcmp(file_output, "abcdef\0\0\0\0", 10)) return 157;
	if (ftruncate(note_fd, 3) || lseek(note_fd, 0, SEEK_CUR) != 6 ||
	    fsync(note_fd) || fstat(note_fd, &fd_metadata) ||
	    fd_metadata.st_size != 3 || close(note_fd))
		return 158;
	read_fd = open("truncate-test.txt", O_RDONLY);
	if (read_fd < 0 || read(read_fd, file_output, sizeof file_output) != 3 ||
	    memcmp(file_output, "abc", 3) || read(read_fd, file_output, 1) != 0 ||
	    close(read_fd) || unlink("truncate-test.txt"))
		return 159;
	errno = 0;
	if (fsync(pipe_fds[0]) != -1 || errno != EBADF) return 160;
	note_fd = open("position-test.txt", O_CREAT | O_WRONLY | O_TRUNC, 0600);
	if (note_fd < 0 || write(note_fd, "0123456789", 10) != 10) return 177;
	if (pwrite(note_fd, "XY", 2, 3) != 2 || pwrite(note_fd, "Z", 1, 12) != 1 ||
	    lseek(note_fd, 0, SEEK_CUR) != 10 || fsync(note_fd))
		return 178;
	read_fd = open("position-test.txt", O_RDONLY);
	if (read_fd < 0 || pread(read_fd, file_output, 2, 3) != 2 ||
	    memcmp(file_output, "XY", 2) || lseek(read_fd, 0, SEEK_CUR) != 0)
		return 179;
	if (pread(read_fd, file_output, 3, 10) != 3 || file_output[0] ||
	    file_output[1] || file_output[2] != 'Z' ||
	    lseek(read_fd, 0, SEEK_CUR) != 0)
		return 180;
	if (read(read_fd, file_output, 10) != 10 ||
	    memcmp(file_output, "012XY56789", 10) ||
	    lseek(read_fd, 0, SEEK_CUR) != 10)
		return 181;
	errno = 0;
	if (pread(pipe_fds[0], file_output, 1, 0) != -1 || errno != ESPIPE)
		return 182;
	if (close(read_fd) || close(note_fd) || unlink("position-test.txt")) return 183;
	note_fd = open("rdwr-test.txt", O_CREAT | O_TRUNC | O_RDWR, 0600);
	if (note_fd < 0 || write(note_fd, "alpha", 5) != 5 ||
	    lseek(note_fd, 0, SEEK_SET) != 0 ||
	    read(note_fd, file_output, 5) != 5 || memcmp(file_output, "alpha", 5))
		return 221;
	if (pwrite(note_fd, "XY", 2, 1) != 2 ||
	    pread(note_fd, file_output, 5, 0) != 5 ||
	    memcmp(file_output, "aXYha", 5) || close(note_fd))
		return 222;
	note_fd = open("rdwr-test.txt", O_RDWR);
	if (note_fd < 0 || fstat(note_fd, &fd_metadata) || fd_metadata.st_size != 5 ||
	    read(note_fd, file_output, 5) != 5 || memcmp(file_output, "aXYha", 5))
		return 223;
	struct flock record_lock = {
		.l_type = F_WRLCK,
		.l_whence = SEEK_SET,
		.l_start = 1,
		.l_len = 2,
	};
	if (fcntl(note_fd, F_SETLKW, &record_lock)) return 224;
	struct flock lock_query = record_lock;
	if (fcntl(note_fd, F_GETLK, &lock_query) || lock_query.l_type != F_UNLCK)
		return 225;
	record_lock.l_type = F_UNLCK;
	if (fcntl(note_fd, F_SETLK, &record_lock) || close(note_fd) ||
	    unlink("rdwr-test.txt"))
		return 226;
	static unsigned char profile_data[8192];
	static unsigned char profile_readback[8192];
	static const char profile_path[] =
		"firefox-profile-storage-long-name-over-thirty-two-bytes.sqlite";
	for (size_t index = 0; index < sizeof profile_data; index++)
		profile_data[index] = (unsigned char)(index * 29u + 7u);
	note_fd = open(profile_path, O_CREAT | O_TRUNC | O_RDWR, 0600);
	if (note_fd < 0 || write(note_fd, profile_data, sizeof profile_data) !=
	    (ssize_t)sizeof profile_data || fsync(note_fd) ||
	    fstat(note_fd, &fd_metadata) ||
	    fd_metadata.st_size != (off_t)sizeof profile_data ||
	    pread(note_fd, profile_readback, sizeof profile_readback, 0) !=
	    (ssize_t)sizeof profile_readback ||
	    memcmp(profile_data, profile_readback, sizeof profile_data) || close(note_fd))
		return 227;
	directory = opendir(".");
	if (!directory) return 228;
	unsigned saw_profile = 0;
	while ((directory_entry = readdir(directory)))
		if (!strcmp(directory_entry->d_name, profile_path) &&
		    directory_entry->d_type == DT_REG)
			saw_profile = 1;
	if (!saw_profile || closedir(directory)) return 229;
	note_fd = open(profile_path, O_RDWR);
	if (note_fd < 0 || pread(note_fd, profile_readback, sizeof profile_readback,
	    0) != (ssize_t)sizeof profile_readback ||
	    memcmp(profile_data, profile_readback, sizeof profile_data) ||
	    close(note_fd) || unlink(profile_path))
		return 230;
	if (!stat("persist/sub/value.txt", &path_metadata)) {
		note_fd = open("persist/sub/value.txt", O_RDONLY);
		if (note_fd < 0 || read(note_fd, file_output, sizeof file_output) != 7 ||
		    memcmp(file_output, "persist", 7) || close(note_fd) ||
		    unlink("persist/sub/value.txt") || rmdir("persist/sub") ||
		    rmdir("persist"))
			return 173;
		persistent_phase = 2;
	} else {
		if (errno != ENOENT || mkdir("persist", 0700) ||
		    mkdir("persist/sub", 0700))
			return 174;
		note_fd = open("persist/sub/value.txt", O_CREAT | O_WRONLY | O_TRUNC, 0600);
		if (note_fd < 0 || write(note_fd, "persist", 7) != 7 ||
		    fsync(note_fd) || close(note_fd))
			return 175;
		persistent_phase = 1;
	}
	if (mkdir("tree", 0700) || mkdir("tree/sub", 0700)) return 161;
	if (stat("tree/sub", &path_metadata) || !S_ISDIR(path_metadata.st_mode))
		return 162;
	if (chdir("tree/sub") || !getcwd(cwd, sizeof cwd) ||
	    strcmp(cwd, "/home/user/tree/sub"))
		return 163;
	note_fd = open("nested.txt", O_CREAT | O_WRONLY | O_TRUNC, 0600);
	if (note_fd < 0 || write(note_fd, "nested", 6) != 6 ||
	    fsync(note_fd) || close(note_fd))
		return 164;
	directory = opendir(".");
	if (!directory || fsync(dirfd(directory))) return 165;
	unsigned saw_nested = 0;
	errno = 0;
	while ((directory_entry = readdir(directory)))
		if (!strcmp(directory_entry->d_name, "nested.txt") &&
		    directory_entry->d_type == DT_REG)
			saw_nested = 1;
	if (errno || !saw_nested || closedir(directory)) return 166;
	if (unlink("nested.txt")) return 167;
	errno = 0;
	if (rmdir(".") != -1 || errno != EBUSY) return 168;
	if (chdir("/home/user")) return 169;
	errno = 0;
	if (rmdir("tree") != -1 || errno != ENOTEMPTY) return 170;
	if (rmdir("tree/sub") || rmdir("tree")) return 171;
	errno = 0;
	if (!stat("tree", &path_metadata) || errno != ENOENT) return 172;
	if (chdir("../../../../") || !getcwd(cwd, sizeof cwd) || strcmp(cwd, "/") ||
	    chdir("/home/user"))
		return 153;
	if (scalable_directory_probe(persistent_phase == 2)) return 253;
	if (fcntl(pipe_fds[0], F_GETFD) != FD_CLOEXEC ||
	    fcntl(pipe_fds[1], F_GETFD) != FD_CLOEXEC)
		return 124;
	readiness[0] = (struct pollfd){.fd = pipe_fds[0], .events = POLLIN};
	readiness[1] = (struct pollfd){.fd = pipe_fds[1], .events = POLLOUT};
	if (poll(readiness, 2, 0) != 1 || readiness[0].revents != 0 ||
	    !(readiness[1].revents & POLLOUT))
		return 125;
	errno = 0;
	if (read(pipe_fds[0], pipe_output, 1) != -1 || errno != EAGAIN)
		return 126;
	for (unsigned index = 0; index < sizeof pipe_input; index++)
		pipe_input[index] = (unsigned char)(index ^ 0xa5);
	if (write(pipe_fds[1], pipe_input, sizeof pipe_input) != sizeof pipe_input)
		return 127;
	errno = 0;
	if (write(pipe_fds[1], pipe_input, 1) != -1 || errno != EAGAIN)
		return 128;
	readiness[0].revents = 0;
	if (poll(readiness, 1, 0) != 1 || !(readiness[0].revents & POLLIN))
		return 129;
	if (read(pipe_fds[0], pipe_output, sizeof pipe_output) != sizeof pipe_output)
		return 130;
	for (unsigned index = 0; index < sizeof pipe_input; index++)
		if (pipe_output[index] != pipe_input[index]) return 131;
	if (close(pipe_fds[1])) return 132;
	readiness[0].revents = 0;
	if (poll(readiness, 1, 0) != 1 ||
	    (readiness[0].revents & (POLLIN | POLLHUP)) != (POLLIN | POLLHUP))
		return 133;
	if (read(pipe_fds[0], pipe_output, 1) != 0 || close(pipe_fds[0]))
		return 134;
	if (pipe2(blocking_pipe, O_CLOEXEC) ||
	    (fcntl(blocking_pipe[0], F_GETFL) & O_NONBLOCK) ||
	    (fcntl(blocking_pipe[1], F_GETFL) & O_NONBLOCK))
		return 191;
	FD_ZERO(&select_read);
	FD_ZERO(&select_write);
	FD_SET(blocking_pipe[0], &select_read);
	FD_SET(blocking_pipe[1], &select_write);
	struct timeval select_now = {0};
	int select_max = blocking_pipe[0] > blocking_pipe[1]
		? blocking_pipe[0] + 1 : blocking_pipe[1] + 1;
	if (select(select_max, &select_read, &select_write, 0, &select_now) != 1 ||
	    FD_ISSET(blocking_pipe[0], &select_read) ||
	    !FD_ISSET(blocking_pipe[1], &select_write))
		return 222;
	winch_action.sa_handler = winch_handler;
	sigemptyset(&winch_action.sa_mask);
	if (sigaction(SIGWINCH, &winch_action, 0) ||
	    sigemptyset(&winch_set) || sigaddset(&winch_set, SIGWINCH) ||
	    sigemptyset(&empty_set) ||
	    pthread_sigmask(SIG_BLOCK, &winch_set, 0) ||
	    makos_call(70, SIGWINCH, 0) || winch_count)
		return 217;
	readiness[0] = (struct pollfd){.fd = blocking_pipe[0], .events = POLLIN};
	struct timespec masked_wait = {.tv_sec = 1, .tv_nsec = 0};
	FD_ZERO(&select_read);
	FD_SET(blocking_pipe[0], &select_read);
	errno = 0;
	if (pselect(blocking_pipe[0] + 1, &select_read, 0, 0,
	    &masked_wait, &empty_set) != -1 || errno != EINTR || winch_count != 1 ||
	    pthread_sigmask(SIG_SETMASK, 0, &observed_set) ||
	    sigismember(&observed_set, SIGWINCH) != 1 ||
	    makos_call(70, SIGWINCH, 0) || winch_count != 1)
		return 223;
	errno = 0;
	if (ppoll(readiness, 1, &masked_wait, &empty_set) != -1 ||
	    errno != EINTR || winch_count != 2 ||
	    pthread_sigmask(SIG_SETMASK, 0, &observed_set) ||
	    sigismember(&observed_set, SIGWINCH) != 1)
		return 218;
	if (pthread_create(&thread, 0, mask_worker, 0) ||
	    pthread_join(thread, &result) || result)
		return 221;
	struct pipe_worker_args worker_args = {
		.read_fd = blocking_pipe[0],
		.write_fd = blocking_pipe[1],
		.read_mode = 0,
	};
	FD_ZERO(&select_read);
	FD_SET(blocking_pipe[0], &select_read);
	masked_wait = (struct timespec){.tv_sec = 0, .tv_nsec = 500000000};
	if (pthread_create(&thread, 0, pipe_worker, &worker_args) ||
	    pselect(blocking_pipe[0] + 1, &select_read, 0, 0,
	    &masked_wait, 0) != 1 || !FD_ISSET(blocking_pipe[0], &select_read) ||
	    pthread_join(thread, &result) || result ||
	    read(blocking_pipe[0], pipe_output, 1) != 1 || pipe_output[0] != 'B')
		return 224;
	FD_ZERO(&select_read);
	FD_SET(blocking_pipe[0], &select_read);
	masked_wait = (struct timespec){.tv_sec = 0, .tv_nsec = 20000000};
	if (pselect(blocking_pipe[0] + 1, &select_read, 0, 0,
	    &masked_wait, 0) != 0 || FD_ISSET(blocking_pipe[0], &select_read))
		return 225;
	FD_ZERO(&select_read);
	FD_SET(255, &select_read);
	masked_wait = (struct timespec){0};
	errno = 0;
	if (pselect(256, &select_read, 0, 0, &masked_wait, 0) != -1 ||
	    errno != EBADF)
		return 226;
	readiness[0] = (struct pollfd){.fd = blocking_pipe[0], .events = POLLIN};
	if (pthread_create(&thread, 0, pipe_worker, &worker_args) ||
	    poll(readiness, 1, 500) != 1 || !(readiness[0].revents & POLLIN) ||
	    pthread_join(thread, &result) || result ||
	    read(blocking_pipe[0], pipe_output, 1) != 1 || pipe_output[0] != 'B')
		return 192;
	if (clock_gettime(CLOCK_MONOTONIC, &sleep_before)) return 193;
	readiness[0].revents = 0;
	if (poll(readiness, 1, 30) != 0 ||
	    clock_gettime(CLOCK_MONOTONIC, &sleep_after))
		return 194;
	elapsed_ns =
		(sleep_after.tv_sec - sleep_before.tv_sec) * 1000000000LL +
		(sleep_after.tv_nsec - sleep_before.tv_nsec);
	if (elapsed_ns < 30000000LL || elapsed_ns > 1000000000LL) return 195;
	if (pthread_create(&thread, 0, pipe_worker, &worker_args) ||
	    read(blocking_pipe[0], pipe_output, 1) != 1 || pipe_output[0] != 'B' ||
	    pthread_join(thread, &result) || result)
		return 196;
	for (unsigned index = 0; index < sizeof pipe_input; index++)
		pipe_input[index] = (unsigned char)index;
	if (write(blocking_pipe[1], pipe_input, sizeof pipe_input) != sizeof pipe_input)
		return 197;
	worker_args.read_mode = 1;
	if (pthread_create(&thread, 0, pipe_worker, &worker_args) ||
	    write(blocking_pipe[1], "W", 1) != 1 ||
	    pthread_join(thread, &result) || result ||
	    read(blocking_pipe[0], pipe_output, 1) != 1 || pipe_output[0] != 'W')
		return 198;
	epoll_fd = epoll_create1(EPOLL_CLOEXEC);
	if (epoll_fd < 0) return 200;
	epoll_events[0] = (struct epoll_event){
		.events = EPOLLIN,
		.data.u64 = 0x50495045,
	};
	if (epoll_ctl(epoll_fd, EPOLL_CTL_ADD, blocking_pipe[0], &epoll_events[0]))
		return 201;
	if (makos_call(70, SIGWINCH, 0) || winch_count != 2) return 219;
	errno = 0;
	if (epoll_pwait(epoll_fd, epoll_events, 2, 1000, &empty_set) != -1 ||
	    errno != EINTR || winch_count != 3 ||
	    pthread_sigmask(SIG_SETMASK, 0, &observed_set) ||
	    sigismember(&observed_set, SIGWINCH) != 1 ||
	    pthread_sigmask(SIG_UNBLOCK, &winch_set, 0))
		return 220;
	if (kill(getpid(), SIGWINCH) || winch_count != 4 || winch_tid != main_tid)
		return 242;
	target_signal_goal = 6;
	if (pthread_create(&thread, 0, target_signal_worker, 0)) return 243;
	while (!target_signal_ready) makos_call(1, 0, 0);
	if (pthread_kill(thread, SIGWINCH)) return 244;
	for (unsigned index = 0; index < 10000 && winch_count < 5; index++)
		makos_call(1, 0, 0);
	if (winch_count != 5 || winch_tid != target_signal_tid) return 245;
	if (syscall(SYS_tgkill, getpid(), target_signal_tid, SIGWINCH)) return 246;
	if (pthread_join(thread, &result) || result || winch_count != 6 ||
	    winch_tid != target_signal_tid)
		return 247;
	errno = 0;
	if (epoll_ctl(epoll_fd, EPOLL_CTL_ADD, blocking_pipe[0], &epoll_events[0]) != -1 ||
	    errno != EEXIST)
		return 202;
	worker_args.read_mode = 0;
	if (pthread_create(&thread, 0, pipe_worker, &worker_args) ||
	    epoll_wait(epoll_fd, epoll_events, 2, 500) != 1 ||
	    epoll_events[0].data.u64 != 0x50495045 ||
	    !(epoll_events[0].events & EPOLLIN) ||
	    pthread_join(thread, &result) || result)
		return 203;
	if (epoll_wait(epoll_fd, epoll_events, 2, 0) != 1 ||
	    read(blocking_pipe[0], pipe_output, 1) != 1 || pipe_output[0] != 'B')
		return 204;
	if (clock_gettime(CLOCK_MONOTONIC, &sleep_before) ||
	    epoll_wait(epoll_fd, epoll_events, 2, 30) != 0 ||
	    clock_gettime(CLOCK_MONOTONIC, &sleep_after))
		return 205;
	elapsed_ns =
		(sleep_after.tv_sec - sleep_before.tv_sec) * 1000000000LL +
		(sleep_after.tv_nsec - sleep_before.tv_nsec);
	if (elapsed_ns < 30000000LL || elapsed_ns > 1000000000LL) return 206;
	epoll_events[0] = (struct epoll_event){
		.events = EPOLLIN | EPOLLET | EPOLLONESHOT,
		.data.u64 = 0x45444745,
	};
	if (epoll_ctl(epoll_fd, EPOLL_CTL_MOD, blocking_pipe[0], &epoll_events[0]) ||
	    pthread_create(&thread, 0, pipe_worker, &worker_args) ||
	    epoll_wait(epoll_fd, epoll_events, 2, 500) != 1 ||
	    epoll_events[0].data.u64 != 0x45444745 ||
	    epoll_wait(epoll_fd, epoll_events, 2, 0) != 0 ||
	    pthread_join(thread, &result) || result)
		return 207;
	epoll_events[0] = (struct epoll_event){
		.events = EPOLLIN | EPOLLET | EPOLLONESHOT,
		.data.u64 = 0x52454152,
	};
	if (epoll_ctl(epoll_fd, EPOLL_CTL_MOD, blocking_pipe[0], &epoll_events[0]) ||
	    epoll_wait(epoll_fd, epoll_events, 2, 0) != 1 ||
	    epoll_events[0].data.u64 != 0x52454152 ||
	    read(blocking_pipe[0], pipe_output, 1) != 1 || pipe_output[0] != 'B' ||
	    epoll_ctl(epoll_fd, EPOLL_CTL_DEL, blocking_pipe[0], 0))
		return 208;
	static const unsigned char dns_query[] = {
		0x4d, 0x4b, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x07, 'e', 'x', 'a', 'm',
		'p', 'l', 'e', 0x03, 'c', 'o', 'm', 0x00, 0x00, 0x01,
		0x00, 0x01,
	};
	struct sockaddr_in dns = {
		.sin_family = AF_INET,
		.sin_port = htons(53),
		.sin_addr.s_addr = htonl(0x0a000203),
	};
	socket_fd = socket(AF_INET, SOCK_DGRAM, 0);
	if (socket_fd < 0 || connect(socket_fd, (struct sockaddr *)&dns, sizeof dns) ||
	    send(socket_fd, dns_query, sizeof dns_query, 0) != sizeof dns_query)
		return 209;
	epoll_events[0] = (struct epoll_event){
		.events = EPOLLIN | EPOLLOUT,
		.data.u64 = 0x534f434b,
	};
	if (epoll_ctl(epoll_fd, EPOLL_CTL_ADD, socket_fd, &epoll_events[0]) ||
	    epoll_wait(epoll_fd, epoll_events, 2, 0) != 1 ||
	    epoll_events[0].data.u64 != 0x534f434b ||
	    !(epoll_events[0].events & EPOLLOUT))
		return 210;
	epoll_events[0] = (struct epoll_event){
		.events = EPOLLIN,
		.data.u64 = 0x534f434b,
	};
	if (epoll_ctl(epoll_fd, EPOLL_CTL_MOD, socket_fd, &epoll_events[0]) ||
	    epoll_wait(epoll_fd, epoll_events, 2, 3000) != 1 ||
	    epoll_events[0].data.u64 != 0x534f434b ||
	    !(epoll_events[0].events & EPOLLIN))
		return 210;
	ssize_t dns_length = recv(socket_fd, pipe_output, sizeof pipe_output, 0);
	if (dns_length <= 0 || !dns_a_record(pipe_output, dns_length, resolved_ip)) return 211;
	epoll_events[0] = (struct epoll_event){
		.events = EPOLLIN,
		.data.u64 = 0x44524149,
	};
	if (epoll_ctl(epoll_fd, EPOLL_CTL_MOD, socket_fd, &epoll_events[0]) ||
	    epoll_wait(epoll_fd, epoll_events, 2, 0) != 0 ||
	    epoll_ctl(epoll_fd, EPOLL_CTL_DEL, socket_fd, 0) ||
	    close(socket_fd))
		return 212;
	struct sockaddr_in6 local6 = {.sin6_family = AF_INET6};
	struct sockaddr_in6 observed6 = {0};
	socklen_t observed6_length = sizeof observed6;
	socklen_t option_length = sizeof(int);
	int option_value = 0;
	socket_fd = socket(AF_INET6, SOCK_DGRAM | SOCK_CLOEXEC, IPPROTO_UDP);
	if (socket_fd < 0 ||
	    getsockopt(socket_fd, SOL_SOCKET, SO_DOMAIN,
		&option_value, &option_length) || option_value != AF_INET6 ||
	    bind(socket_fd, (struct sockaddr *)&local6, sizeof local6) ||
	    getsockname(socket_fd, (struct sockaddr *)&observed6,
		&observed6_length) || observed6_length != sizeof observed6 ||
	    observed6.sin6_family != AF_INET6 || !observed6.sin6_port)
		return 231;
	option_value = 0;
	option_length = sizeof option_value;
	if (getsockopt(socket_fd, IPPROTO_IPV6, IPV6_V6ONLY,
		&option_value, &option_length) || option_value != 1 ||
	    setsockopt(socket_fd, IPPROTO_IPV6, IPV6_V6ONLY,
		&(int){1}, sizeof(int)) || close(socket_fd))
		return 232;
	struct sockaddr_in6 dns6 = {
		.sin6_family = AF_INET6,
		.sin6_port = htons(53),
		.sin6_addr.s6_addr = {0xfe, 0xc0, [15] = 3},
	};
	socket_fd = socket(AF_INET6, SOCK_DGRAM, IPPROTO_UDP);
	readiness[0] = (struct pollfd){.fd = socket_fd, .events = POLLIN};
	if (socket_fd < 0 ||
	    connect(socket_fd, (struct sockaddr *)&dns6, sizeof dns6) ||
	    send(socket_fd, dns_query, sizeof dns_query, 0) != sizeof dns_query)
		return 235;
	int udp6_ready = poll(readiness, 1, 3000);
	if (udp6_ready < 0 ||
	    (udp6_ready == 1 && !(readiness[0].revents & POLLIN))) return 235;
	if (udp6_ready == 1) {
		dns_length = recv(socket_fd, pipe_output, sizeof pipe_output, 0);
		if (dns_length <= 0 || !dns_a_record(pipe_output, dns_length, resolved_ip))
			return 236;
	} else {
		static const char udp6_limited[] =
			"MAKOS_MUSL_IPV6_BACKEND_LIMITED backend=qemu-usernet "
			"reason=udp6-dns-timeout dns=fec0::3 tx=verified rx=unverified\n";
		if (write(1, udp6_limited, sizeof udp6_limited - 1) !=
		    sizeof udp6_limited - 1) return 236;
	}
	if (close(socket_fd)) return 236;
	struct addrinfo resolver6_hints = {
		.ai_family = AF_INET6,
		.ai_socktype = SOCK_STREAM,
	};
	struct addrinfo *resolver6_results = 0;
	if (getaddrinfo("example.com", "443", &resolver6_hints,
		&resolver6_results))
		return 233;
	int resolver_ipv6 = 0;
	for (struct addrinfo *entry = resolver6_results; entry; entry = entry->ai_next)
		if (entry->ai_family == AF_INET6 &&
		    entry->ai_addrlen >= sizeof(struct sockaddr_in6))
			resolver_ipv6 = 1;
	freeaddrinfo(resolver6_results);
	if (!resolver_ipv6) return 234;
	struct addrinfo resolver_hints = {
		.ai_family = AF_UNSPEC,
		.ai_socktype = SOCK_STREAM,
		.ai_flags = AI_ADDRCONFIG,
	};
	struct addrinfo *resolver_results = 0;
	int resolver_status = getaddrinfo(
		"example.com", "80", &resolver_hints, &resolver_results);
	if (resolver_status) {
		static const char resolver_system[] =
			"MAKOS_MUSL_RESOLVER_FAIL status=EAI_SYSTEM\n";
		static const char resolver_again[] =
			"MAKOS_MUSL_RESOLVER_FAIL status=EAI_AGAIN\n";
		static const char resolver_noname[] =
			"MAKOS_MUSL_RESOLVER_FAIL status=EAI_NONAME\n";
		static const char resolver_other[] =
			"MAKOS_MUSL_RESOLVER_FAIL status=other\n";
		const char *failure = resolver_status == EAI_SYSTEM ? resolver_system
			: resolver_status == EAI_AGAIN ? resolver_again
			: resolver_status == EAI_NONAME ? resolver_noname : resolver_other;
		(void)write(1, failure, strlen(failure));
		return 228;
	}
	int resolver_ipv4 = 0;
	for (struct addrinfo *entry = resolver_results; entry; entry = entry->ai_next)
		if (entry->ai_family == AF_INET &&
		    entry->ai_addrlen >= sizeof(struct sockaddr_in))
			resolver_ipv4 = 1;
	freeaddrinfo(resolver_results);
	if (!resolver_ipv4) return 229;
	struct sockaddr_in web = {
		.sin_family = AF_INET,
		.sin_port = htons(80),
	};
	memcpy(&web.sin_addr.s_addr, resolved_ip, 4);
	socket_fd = socket(AF_INET, SOCK_STREAM, 0);
	static const char http_request[] =
		"GET / HTTP/1.0\r\nHost: example.com\r\nConnection: close\r\n\r\n";
	if (socket_fd < 0 || connect(socket_fd, (struct sockaddr *)&web, sizeof web) ||
	    send(socket_fd, http_request, sizeof http_request - 1, 0) !=
		(ssize_t)(sizeof http_request - 1))
		return 213;
	epoll_events[0] = (struct epoll_event){
		.events = EPOLLIN | EPOLLRDHUP,
		.data.u64 = 0x54435052,
	};
	if (epoll_ctl(epoll_fd, EPOLL_CTL_ADD, socket_fd, &epoll_events[0]) ||
	    epoll_wait(epoll_fd, epoll_events, 2, 3000) != 1 ||
	    epoll_events[0].data.u64 != 0x54435052 ||
	    !(epoll_events[0].events & EPOLLIN))
		return 214;
	ssize_t http_length = recv(socket_fd, pipe_output, sizeof pipe_output, 0);
	if (http_length < 12 || memcmp(pipe_output, "HTTP/1.", 7)) return 215;
	if (epoll_ctl(epoll_fd, EPOLL_CTL_DEL, socket_fd, 0) ||
	    close(socket_fd) || close(epoll_fd))
		return 216;
	if (close(blocking_pipe[1]) || close(blocking_pipe[0])) return 199;
	unsigned char *advice_page = mmap(0, 4096, PROT_READ | PROT_WRITE,
		MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	if (advice_page == MAP_FAILED) return 217;
	memset(advice_page, 0xa5, 4096);
	if (madvise(advice_page, 4096, MADV_DONTNEED)) return 218;
	for (size_t index = 0; index < 4096; index++)
		if (advice_page[index] != 0) return 219;
	memset(advice_page, 0x5a, 4096);
	if (madvise(advice_page, 4096, MADV_FREE)) return 237;
	for (size_t index = 0; index < 4096; index++)
		if (advice_page[index] != 0) return 238;
	if (munmap(advice_page, 4096)) return 220;
	static const char shared_name[] = "/makos-firefox-shmem-probe";
	int shared_fd = shm_open(shared_name, O_CREAT | O_EXCL | O_RDWR, 0600);
	if (shared_fd < 0 || ftruncate(shared_fd, 8192)) return 221;
	int shared_read_fd = shm_open(shared_name, O_RDONLY, 0);
	if (shared_read_fd < 0 || shm_unlink(shared_name)) return 222;
	unsigned char *shared_write = mmap(0, 8192, PROT_READ | PROT_WRITE,
		MAP_SHARED, shared_fd, 0);
	unsigned char *shared_read = mmap(0, 8192, PROT_READ,
		MAP_SHARED, shared_read_fd, 0);
	if (shared_write == MAP_FAILED || shared_read == MAP_FAILED) return 223;
	shared_write[17] = 0x4d;
	shared_write[4097] = 0x6b;
	if (shared_read[17] != 0x4d || shared_read[4097] != 0x6b) return 224;
	if (munmap(shared_write, 8192) || close(shared_fd) ||
	    shared_read[17] != 0x4d || munmap(shared_read, 8192) ||
	    close(shared_read_fd)) return 225;
	errno = 0;
	if (shm_open(shared_name, O_RDONLY, 0) >= 0 || errno != ENOENT) return 226;
	pthread_t exit_group_thread;
	if (pthread_mutex_init(&timed_mutex, 0) ||
	    pthread_create(&thread, 0, timed_mutex_worker, 0))
		return 248;
	while (!timed_mutex_ready) makos_call(1, 0, 0);
	struct timespec timed_deadline;
	if (clock_gettime(CLOCK_REALTIME, &timed_deadline) ||
	    clock_gettime(CLOCK_MONOTONIC, &sleep_before))
		return 249;
	timed_deadline.tv_nsec += 50000000;
	if (timed_deadline.tv_nsec >= 1000000000) {
		timed_deadline.tv_sec++;
		timed_deadline.tv_nsec -= 1000000000;
	}
	if (pthread_mutex_timedlock(&timed_mutex, &timed_deadline) != ETIMEDOUT ||
	    clock_gettime(CLOCK_MONOTONIC, &sleep_after))
		return 250;
	elapsed_ns =
		(sleep_after.tv_sec - sleep_before.tv_sec) * 1000000000LL +
		(sleep_after.tv_nsec - sleep_before.tv_nsec);
	if (elapsed_ns < 50000000LL || elapsed_ns > 1000000000LL) return 251;
	timed_mutex_release = 1;
	if (pthread_join(thread, &result) || result || pthread_mutex_destroy(&timed_mutex))
		return 252;
	pthread_t requeue_threads[3];
	if (pthread_mutex_init(&requeue_mutex, 0) ||
	    pthread_cond_init(&requeue_condition, 0))
		return 184;
	for (unsigned index = 0; index < 3; index++)
		if (pthread_create(&requeue_threads[index], 0, requeue_worker, 0))
			return 184;
	for (unsigned attempts = 0; attempts < 10000; attempts++) {
		unsigned ready;
		if (pthread_mutex_lock(&requeue_mutex)) return 184;
		ready = requeue_ready;
		if (pthread_mutex_unlock(&requeue_mutex)) return 184;
		if (ready == 3) break;
		makos_call(1, 0, 0);
	}
	if (pthread_mutex_lock(&requeue_mutex)) return 184;
	if (requeue_ready != 3) return 184;
	requeue_release = 1;
	if (pthread_cond_broadcast(&requeue_condition) ||
	    pthread_mutex_unlock(&requeue_mutex))
		return 184;
	for (unsigned attempts = 0; attempts < 10000; attempts++) {
		unsigned completed;
		if (pthread_mutex_lock(&requeue_mutex)) return 184;
		completed = requeue_completed;
		if (pthread_mutex_unlock(&requeue_mutex)) return 184;
		if (completed == 3) break;
		makos_call(1, 0, 0);
	}
	if (requeue_completed != 3) return 184;
	for (unsigned index = 0; index < 3; index++)
		if (pthread_join(requeue_threads[index], &result) || result)
			return 184;
	if (pthread_cond_destroy(&requeue_condition) ||
	    pthread_mutex_destroy(&requeue_mutex))
		return 184;
	static const char requeue_marker[] =
		"MAKOS_AARCH64_FUTEX_REQUEUE_OK libc=pthread_cond_broadcast "
		"waiters=3 wake=relay requeue=mutex fifo=1 joins=bounded\n";
	if (write(1, requeue_marker, sizeof requeue_marker - 1) !=
	    sizeof requeue_marker - 1)
		return 184;
	if (pthread_create(&exit_group_thread, 0, exit_group_worker, 0)) return 227;
	while (!exit_group_worker_ready) makos_call(1, 0, 0);
	pthread_mutexattr_t robust_attribute;
	if (pthread_mutexattr_init(&robust_attribute) ||
	    pthread_mutexattr_setrobust(&robust_attribute, PTHREAD_MUTEX_ROBUST) ||
	    pthread_mutexattr_destroy(&robust_attribute))
		return 238;
	if (pthread_create(&thread, 0, robust_worker, 0)) return 239;
	while (!robust_worker_ready) makos_call(1, 0, 0);
	unsigned robust_observed = robust_node.futex;
	robust_waiting = 1;
	if (syscall(SYS_futex, &robust_node.futex, 0, robust_observed, 0, 0, 0))
		return 240;
	if (robust_node.futex != 0xc0000000U) return 241;
	static const char directory_create_marker[] =
		"MAKOS_MAKFS_DIRECTORY_PERSIST_OK phase=create format=makfs4 "
		"path=/home/user/persist/sub/value.txt scalable=64-siblings,name255,indexed-lookup,cursor\n";
	static const char directory_remount_marker[] =
		"MAKOS_MAKFS_DIRECTORY_PERSIST_OK phase=remount-read-cleanup format=makfs4 "
		"path=/home/user/persist/sub/value.txt scalable=64-siblings,name255,indexed-lookup,cursor\n";
	const char *directory_marker = persistent_phase == 1
		? directory_create_marker : directory_remount_marker;
	size_t directory_marker_length = persistent_phase == 1
		? sizeof directory_create_marker - 1 : sizeof directory_remount_marker - 1;
	if (write(1, directory_marker, directory_marker_length) != directory_marker_length)
		return 176;
	static const char directory_scale_marker[] =
		"MAKOS_MAKFS_DIRECTORY_SCALE_OK siblings=64 name_bytes=255 "
		"lookup=hash-index cursor=resumable remount=verified\n";
	if (write(1, directory_scale_marker, sizeof directory_scale_marker - 1) !=
	    sizeof directory_scale_marker - 1)
		return 253;
	static const char marker[] =
		"MAKOS_MUSL_PTHREAD_OK version=1.2.6 clone=shared-vm "
		"tls=distinct futex=wait,wake,requeue,timed-timeout join=1 clear_child_tid=1 robust=owner-death,wake-one "
		"ipc=channel,event blocking=1 cleanup=handles "
		"getrandom=virtio-rng bytes=64 zeroized=1 "
		"clock_realtime=pl031 tls_validation=ready "
		"sleep=nanosleep,clock_nanosleep,blocked,timer-wake resolution=10ms "
		"process_identity=pid,uid,gid session_bound=1 "
		"parent=thread-consistent "
		"pipe=blocking,nonblock,cloexec,bounded,atomic "
		"poll=timed,block,wake,timeout,write,read,eagain,eof,hup "
		"signals=task-mask,inherit,pselect-atomic,ppoll-atomic,epoll-pwait-atomic,eintr,restore,kill,tkill,tgkill "
		"select=fdsets,timeout,pipe-wake,ebadf "
		"epoll=create,ctl,level,edge,oneshot,pipe,udp,tcp-async,timed "
		"resolver=getaddrinfo,addrconfig,ipv4,ipv6,aaaa,udp4 udp6=tx,rx-backend-dependent "
		"metadata=stat,fstat,regular,fifo,tty,timestamps-unix "
		"symlink=create,readlink,lstat,follow,readdir,unlink "
		"directory=opendir,getdents64,readdir,rewind,dot,dotdot "
		"cwd=getcwd,chdir,relative,dotdot,create,rename,replace,unlink "
		"file_access=rdwr,preserve "
		"record_lock=setlkw,getlk-own,unlock "
		"profile_storage=makfs4,8k,long-name,readdir,reopen "
		"file_size=ftruncate,grow-zero,shrink,offset-unchanged "
		"durability=virtio-flush,fsync "
		"positional=pread,pwrite,offset-preserved,sparse-zero "
		"madvise=dontneed,free,decommit,zero-refault "
		"shmem=posix,named,excl,truncate,unlink-lifetime,shared-coherent,readonly,reclaim "
		"directories=mkdir,nested,stat,readdir,rmdir,notempty,busy "
		"exit_group=all-threads\n";
	if (write(1, marker, sizeof marker - 1) != sizeof marker - 1) return 113;
	return 42;
}
