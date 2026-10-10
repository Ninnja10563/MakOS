/* SPDX-License-Identifier: MIT */
#define _GNU_SOURCE
#include <fcntl.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/syscall.h>
#include <unistd.h>

enum { WORKERS = 3, RX_CALLS = 32, PAGE_BYTES = 4096 };
enum { FAULT_ROUNDS = 16, TABLE_BYTES = 2 * 1024 * 1024,
	FAULT_BYTES = (FAULT_ROUNDS * 2 + 1) * TABLE_BYTES };
static atomic_uint ready_workers;
static atomic_uint failed_worker;
static atomic_uint fault_arrivals;
static atomic_uint fault_generation;

struct worker_result {
	unsigned cpu;
	long tid;
	unsigned completed;
	unsigned fault_rounds;
	uintptr_t fault_base;
	int (*resume_probe)(void);
};

static int emit_el0_result(const struct worker_result results[WORKERS],
	uintptr_t thread_entry, void *code)
{
	char record[512];
	int length = snprintf(record, sizeof record,
		"MAKOS_MUSL_EL0_ENTRY_OK loader=musl threads=3 "
		"singleton=0x2,0x4,0x8 tids=%ld,%ld,%ld pthread_create=%p "
		"rx=%p resume=%p calls=96 statuses=42,42,42 block=sleep-until\n",
		results[0].tid, results[1].tid, results[2].tid,
		(void *)thread_entry, code, (char *)code + PAGE_BYTES);
	if (length <= 0 || (size_t)length >= sizeof record)
		return -1;
	/* stdio/writev can split a record across native writes. Publish this
	 * complete bounded record once; a partial/error result is a failure,
	 * not permission to retry a suffix around another CPU's diagnostics. */
	return write(STDOUT_FILENO, record, (size_t)length) == (ssize_t)length
		? 0 : -1;
}

static int emit_fault_result(const struct worker_result results[WORKERS])
{
	char record[512];
	int length = snprintf(record, sizeof record,
		"MAKOS_MUSL_VM_FAULT_OK loader=musl threads=3 "
		"singleton=0x2,0x4,0x8 tids=%ld,%ld,%ld rounds=16 "
		"same_page_rounds=16 distinct_pages=48 table_stride=2097152 "
		"base=%p coherent_checks=288 first_touch=barrier-released "
		"statuses=42,42,42 cleanup=unmapped\n",
		results[0].tid, results[1].tid, results[2].tid,
		(void *)results[0].fault_base);
	if (length <= 0 || (size_t)length >= sizeof record)
		return -1;
	return write(STDOUT_FILENO, record, (size_t)length) == (ssize_t)length
		? 0 : -1;
}

static long native_user_write(unsigned number, const void *buffer, size_t length)
{
	register uintptr_t x0 __asm__("x0") = STDERR_FILENO;
	register uintptr_t x1 __asm__("x1") = (uintptr_t)buffer;
	register uintptr_t x2 __asm__("x2") = length;
	register uintptr_t x8 __asm__("x8") = number;
	__asm__ volatile("svc #0" : "+r"(x0)
		: "r"(x1), "r"(x2), "r"(x8) : "memory", "cc");
	return (long)x0;
}

static int user_write_failure(unsigned phase, long observed)
{
	char record[192];
	int length = snprintf(record, sizeof record,
		"MAKOS_MUSL_USER_WRITE_FAIL phase=%u observed=%ld\n", phase, observed);
	if (length > 0 && (size_t)length < sizeof record)
		(void)write(STDERR_FILENO, record, (size_t)length);
	return -1;
}

static int emit_user_write_result(void)
{
	char record[512];
	int length = snprintf(record, sizeof record,
		"MAKOS_MUSL_USER_WRITE_OK source=immutable-mmap "
		"first_touch=kernel-copy tty=musl-17,native-63 tty_records=2 "
		"file_offset=4093 file_bytes=67 readback=exact negatives=10 "
		"errors=17:-1,63:-22 cleanup=unmapped,unlinked\n");
	if (length <= 0 || (size_t)length >= sizeof record)
		return -1;
	return write(STDOUT_FILENO, record, (size_t)length) == (ssize_t)length
		? 0 : -1;
}

static int reject_user_write(unsigned phase, const void *buffer, size_t length)
{
	/* Preserve MakOS's current distinct native ABI errors. Musl translates
	 * native write's -1 to EBADF; this regression does not redefine errno. */
	long observed = native_user_write(17, buffer, length);
	if (observed != -1)
		return user_write_failure(phase, observed);
	observed = native_user_write(63, buffer, length);
	if (observed != -22)
		return user_write_failure(phase + 1, observed);
	return 0;
}

static int probe_user_write(void)
{
	const size_t text_bytes = sizeof "#include <stdint.h>\n" - 1;
	char begin[128];
	int length = snprintf(begin, sizeof begin,
		"MAKOS_MUSL_USER_WRITE_BEGIN source=immutable-mmap tty_records=2\n");
	if (length <= 0 || (size_t)length >= sizeof begin ||
	    write(STDOUT_FILENO, begin, (size_t)length) != (ssize_t)length)
		return user_write_failure(1, length);
	int source = open("/usr/src/makos/ports/musl/shared-demo.c", O_RDONLY);
	if (source < 0)
		return user_write_failure(2, source);
	for (unsigned path = 0; path < 2; path++) {
		void *cold = mmap(0, PAGE_BYTES, PROT_READ, MAP_PRIVATE, source, 0);
		if (cold == MAP_FAILED)
			return user_write_failure(3 + path * 3, -1);
		/* No memcpy, strlen, comparison, volatile access or warm-up read:
		 * each new immutable-file VMA first reaches its bytes in the kernel.
		 * Normal musl write dispatches native 17; test native 63 separately. */
		long written = path == 0 ? write(STDERR_FILENO, cold, text_bytes)
			: native_user_write(63, cold, text_bytes);
		if (written != (long)text_bytes)
			return user_write_failure(4 + path * 3, written);
		if (munmap(cold, PAGE_BYTES))
			return user_write_failure(5 + path * 3, -1);
	}
	if (close(source))
		return user_write_failure(9, -1);

	source = open("/usr/lib/libc.so", O_RDONLY);
	if (source < 0)
		return user_write_failure(10, source);
	void *cold = mmap(0, PAGE_BYTES * 2, PROT_READ, MAP_PRIVATE, source, 0);
	if (cold == MAP_FAILED)
		return user_write_failure(11, -1);
	char path[64];
	length = snprintf(path, sizeof path, "/home/user/cold-write-%ld", (long)getpid());
	if (length <= 0 || (size_t)length >= sizeof path)
		return user_write_failure(12, length);
	int target = open(path, O_CREAT | O_EXCL | O_RDWR, 0600);
	if (target < 0)
		return user_write_failure(13, target);
	/* This source view is still untouched. The span crosses two cold file
	 * pages, and the bytes are checked against independent pread afterwards. */
	long written = write(target, (const char *)cold + PAGE_BYTES - 3, 67);
	char expected[67], actual[67];
	int failed = written != 67 ||
		pread(source, expected, sizeof expected, PAGE_BYTES - 3) != (ssize_t)sizeof expected ||
		pread(target, actual, sizeof actual, 0) != (ssize_t)sizeof actual ||
		memcmp(expected, actual, sizeof actual);
	/* Only our successfully O_EXCL-created file may be removed. */
	int close_result = close(target);
	int unlink_result = unlink(path);
	int unmap_result = munmap(cold, PAGE_BYTES * 2);
	int source_result = close(source);
	if (failed || close_result || unlink_result || unmap_result || source_result)
		return user_write_failure(14, written);

	void *none = mmap(0, PAGE_BYTES, PROT_NONE,
		MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	if (none == MAP_FAILED || reject_user_write(20, none, 8) ||
	    munmap(none, PAGE_BYTES))
		return user_write_failure(22, -1);
	void *resident = mmap(0, PAGE_BYTES, PROT_READ | PROT_WRITE,
		MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	if (resident == MAP_FAILED)
		return user_write_failure(23, -1);
	*(volatile unsigned char *)resident = 'X';
	if (mprotect(resident, PAGE_BYTES, PROT_NONE) ||
	    reject_user_write(24, resident, 8) || munmap(resident, PAGE_BYTES))
		return user_write_failure(26, -1);
	void *hole = mmap(0, PAGE_BYTES * 3, PROT_READ | PROT_WRITE,
		MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	if (hole == MAP_FAILED)
		return user_write_failure(27, -1);
	memcpy((char *)hole + PAGE_BYTES - 8, "UNSAFE!!", 8);
	if (munmap((char *)hole + PAGE_BYTES, PAGE_BYTES) ||
	    reject_user_write(28, (char *)hole + PAGE_BYTES - 8, 16) ||
	    munmap(hole, PAGE_BYTES) ||
	    munmap((char *)hole + PAGE_BYTES * 2, PAGE_BYTES))
		return user_write_failure(30, -1);
	if (reject_user_write(31, hole, 8) ||
	    reject_user_write(33, (const void *)(UINTPTR_MAX - 7), 16))
		return -1;
	return emit_user_write_result();
}

/* The last arrival publishes every participant's writes before releasing
 * the next generation. Each worker is pinned to a different application
 * processor. This is a real access race, not proof that every round takes
 * the losing-fault kernel path: that interleaving has a separate host test. */
static int fault_barrier(void)
{
	unsigned generation = atomic_load_explicit(&fault_generation,
		memory_order_acquire);
	if (atomic_fetch_add_explicit(&fault_arrivals, 1,
		memory_order_acq_rel) == WORKERS - 1) {
		atomic_store_explicit(&fault_arrivals, 0, memory_order_relaxed);
		atomic_fetch_add_explicit(&fault_generation, 1, memory_order_release);
	} else {
		while (atomic_load_explicit(&fault_generation,
			memory_order_acquire) == generation) {
			if (atomic_load_explicit(&failed_worker, memory_order_acquire))
				return -1;
			sched_yield();
		}
	}
	return atomic_load_explicit(&failed_worker, memory_order_acquire) ? -1 : 0;
}

static uint64_t fault_value(unsigned round, unsigned worker)
{
	return ((uint64_t)(round + 1) << 32) | (worker + 1);
}

static int concurrent_first_touches(struct worker_result *result)
{
	unsigned worker = result->cpu - 1;
	for (unsigned round = 0; round < FAULT_ROUNDS; round++) {
		uintptr_t base = result->fault_base + (uintptr_t)round * 2 * TABLE_BYTES;
		volatile uint64_t *shared = (volatile uint64_t *)base;
		volatile uint64_t *distinct = (volatile uint64_t *)(base + TABLE_BYTES);
		if (fault_barrier())
			return -1;
		/* The parent never touches this reservation. Disjoint words on
		 * one fresh page exercise competing first faults to one leaf. */
		shared[worker] = fault_value(round, worker);
		if (fault_barrier())
			return -1;
		for (unsigned peer = 0; peer < WORKERS; peer++)
			if (shared[peer] != fault_value(round, peer))
				return -1;
		if (fault_barrier())
			return -1;
		/* A different untouched 2 MiB subtree tests concurrent table
		 * creation for distinct leaves, not just a shared-page winner. */
		distinct[worker * PAGE_BYTES / sizeof *distinct] =
			fault_value(round, worker) ^ UINT64_C(0x5a5a000000000000);
		if (fault_barrier())
			return -1;
		for (unsigned peer = 0; peer < WORKERS; peer++)
			if (distinct[peer * PAGE_BYTES / sizeof *distinct] !=
			    (fault_value(round, peer) ^ UINT64_C(0x5a5a000000000000)))
				return -1;
		result->fault_rounds++;
	}
	return 0;
}

static void *dynamic_worker(void *argument)
{
	struct worker_result *result = argument;
	cpu_set_t requested, observed;
	CPU_ZERO(&requested);
	CPU_SET(result->cpu, &requested);
	if (sched_setaffinity(0, sizeof requested, &requested) ||
	    sched_getaffinity(0, sizeof observed, &observed) ||
	    CPU_COUNT(&observed) != 1 || !CPU_ISSET(result->cpu, &observed)) {
		atomic_store_explicit(&failed_worker, 1, memory_order_release);
		return (void *)(uintptr_t)123;
	}
	result->tid = syscall(SYS_gettid);
	if (result->tid <= 0) {
		atomic_store_explicit(&failed_worker, 1, memory_order_release);
		return (void *)(uintptr_t)124;
	}
	/* Do not enter high code until all three threads are bound to their APs. */
	atomic_fetch_add_explicit(&ready_workers, 1, memory_order_acq_rel);
	while (atomic_load_explicit(&ready_workers, memory_order_acquire) != WORKERS) {
		if (atomic_load_explicit(&failed_worker, memory_order_acquire))
			return (void *)(uintptr_t)123;
		sched_yield();
	}
	if (concurrent_first_touches(result)) {
		atomic_store_explicit(&failed_worker, 1, memory_order_release);
		return (void *)(uintptr_t)134;
	}
	for (unsigned index = 0; index < RX_CALLS; index++) {
		if (result->resume_probe() != 42)
			return (void *)(uintptr_t)125;
		result->completed++;
	}
	return (void *)(uintptr_t)42;
}

int main(int argc, char **argv)
{
	static const char marker[] =
		"MAKOS_MUSL_DYNAMIC_OK loader=musl relocations=executed\n";
	if (argc != 2 || strcmp(argv[0], "/system/musl-dynamic-probe") ||
	    strcmp(argv[1], "dynamic"))
		return 121;
	/*
	 * This is a genuinely dynamic musl program. Its pthread clone starts
	 * inside the mapped interpreter, outside the main-image load window.
	 * No kernel test role or synthetic user context creates these threads.
	 */
	uintptr_t thread_entry = (uintptr_t)pthread_create;
	if (thread_entry < UINT64_C(0x28000000) ||
	    thread_entry >= UINT64_C(0x30000000))
		return 126;

	uint32_t *code = mmap(0, PAGE_BYTES * 2, PROT_READ | PROT_WRITE,
		MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	if (code == MAP_FAILED)
		return 127;
	if ((uintptr_t)code < UINT64_C(0x80000000) ||
	    (uintptr_t)code % PAGE_BYTES)
		return 128;
	/*
	 * Native MakOS clock_ticks (27), then sleep_until (103) five ticks later.
	 * Unlike a yield with no ready peer on a singleton CPU, this blocks and
	 * returns through the outer scheduler's validated EL0-entry path.
	 * Place the sleep SVC in the final slot of one executable page; its saved
	 * resume PC is on the next high-address executable page.
	 * Both pages are populated before RW -> RX; there is never a W+X VMA.
	 */
	code[PAGE_BYTES / sizeof *code - 5] = UINT32_C(0xd2800368); /* mov x8,#27 */
	code[PAGE_BYTES / sizeof *code - 4] = UINT32_C(0xd4000001); /* svc #0 */
	code[PAGE_BYTES / sizeof *code - 3] = UINT32_C(0x91001400); /* add x0,x0,#5 */
	code[PAGE_BYTES / sizeof *code - 2] = UINT32_C(0xd2800ce8); /* mov x8,#103 */
	code[PAGE_BYTES / sizeof *code - 1] = UINT32_C(0xd4000001);
	code[PAGE_BYTES / sizeof *code] = UINT32_C(0x52800540);
	code[PAGE_BYTES / sizeof *code + 1] = UINT32_C(0xd65f03c0);
	/* MakOS mprotect synchronizes resident executable pages' I/D caches. */
	if (mprotect(code, PAGE_BYTES * 2, PROT_READ | PROT_EXEC))
		return 129;

	/* Reserve virtual space only: 16 shared pages and 48 distinct pages
	 * will demand 256 KiB of physical storage, plus page tables. Starting
	 * each phase in a fresh aligned 2 MiB subtree exposes table publication
	 * races as well as duplicate leaf installation. */
	void *fault_region = mmap(0, FAULT_BYTES, PROT_READ | PROT_WRITE,
		MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	if (fault_region == MAP_FAILED)
		return 135;
	uintptr_t fault_base = ((uintptr_t)fault_region + TABLE_BYTES - 1) &
		~(uintptr_t)(TABLE_BYTES - 1);
	if (fault_base < UINT64_C(0x80000000) ||
	    fault_base + (uintptr_t)FAULT_ROUNDS * 2 * TABLE_BYTES >
		(uintptr_t)fault_region + FAULT_BYTES)
		return 136;

	pthread_t threads[WORKERS];
	struct worker_result results[WORKERS] = {0};
	for (unsigned index = 0; index < WORKERS; index++) {
		results[index].cpu = index + 1;
		results[index].fault_base = fault_base;
		results[index].resume_probe = (int (*)(void))
			((char *)code + PAGE_BYTES - 20);
		if (pthread_create(&threads[index], 0, dynamic_worker, &results[index]))
			return 130;
	}
	for (unsigned index = 0; index < WORKERS; index++) {
		void *status = 0;
		if (pthread_join(threads[index], &status) ||
		    (uintptr_t)status != 42 || results[index].completed != RX_CALLS ||
		    results[index].fault_rounds != FAULT_ROUNDS ||
		    results[index].tid == syscall(SYS_gettid))
			return 131;
		for (unsigned previous = 0; previous < index; previous++)
			if (results[index].tid == results[previous].tid)
				return 132;
	}
	if (emit_el0_result(results, thread_entry, code) ||
	    munmap(code, PAGE_BYTES * 2))
		return 133;
	if (munmap(fault_region, FAULT_BYTES) || emit_fault_result(results))
		return 137;
	if (probe_user_write())
		return 138;
	if (write(1, marker, sizeof marker - 1) != sizeof marker - 1)
		return 122;
	return 42;
}
