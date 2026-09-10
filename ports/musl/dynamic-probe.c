/* SPDX-License-Identifier: MIT */
#define _GNU_SOURCE
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
static atomic_uint ready_workers;
static atomic_uint failed_worker;

struct worker_result {
	unsigned cpu;
	long tid;
	unsigned completed;
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

	pthread_t threads[WORKERS];
	struct worker_result results[WORKERS] = {0};
	for (unsigned index = 0; index < WORKERS; index++) {
		results[index].cpu = index + 1;
		results[index].resume_probe = (int (*)(void))
			((char *)code + PAGE_BYTES - 20);
		if (pthread_create(&threads[index], 0, dynamic_worker, &results[index]))
			return 130;
	}
	for (unsigned index = 0; index < WORKERS; index++) {
		void *status = 0;
		if (pthread_join(threads[index], &status) ||
		    (uintptr_t)status != 42 || results[index].completed != RX_CALLS ||
		    results[index].tid == syscall(SYS_gettid))
			return 131;
		for (unsigned previous = 0; previous < index; previous++)
			if (results[index].tid == results[previous].tid)
				return 132;
	}
	if (emit_el0_result(results, thread_entry, code) ||
	    munmap(code, PAGE_BYTES * 2))
		return 133;
	if (write(1, marker, sizeof marker - 1) != sizeof marker - 1)
		return 122;
	return 42;
}
