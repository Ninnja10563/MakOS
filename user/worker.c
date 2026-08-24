#include <makos.h>

static size_t text_length(const char *text) {
    size_t length = 0;
    while (text[length] != '\0')
        ++length;
    return length;
}

static void fill(long surface, uint16_t x, uint16_t y, uint16_t width,
                 uint16_t height, uint32_t color) {
    if (makos_surface_fill(surface, x, y, width, height, color) != 1)
        makos_exit(124);
}

static void worker_thread(void *argument) {
    volatile uint64_t *shared = (volatile uint64_t *)argument;
    makos_write("MakOS C user thread running\n",
                sizeof("MakOS C user thread running\n") - 1);
    shared[0] = UINT64_C(0x53594e434f4b);
    if (makos_event_signal((long)shared[1]) != 1)
        makos_thread_exit(8);
    makos_thread_exit(7);
}

void _start(void) {
    static const char path[] = "/boot-count.txt";
    static const char running[] = "MakOS C worker process running\n";
    static const char passed[] =
        "MAKOS_POSIX_OK app=c-worker libc=static write=1 open=1 read=1 close=1\n";
    char bytes[32];
    long fd;
    long count;
    long surface;
    uint64_t channels[2];
    volatile uint64_t *mapping;
    long tid;
    long event;
    long thread_status = -1;
    volatile uint8_t *large;
    volatile uint8_t *second;
    volatile uint8_t *leaked;
    volatile uint8_t *reused;
    struct makos_spawn_arguments spawn_arguments;
    const char *spawn_argv[] = {"/home/user/generated.elf", "worker"};
    const char *spawn_envp[] = {"MODE=sandbox"};

    makos_write(running, text_length(running));
    if (makos_abi_info(0) != MAKOS_ABI_VERSION ||
        (makos_abi_info(2) &
         (MAKOS_FEATURE_SYNC | MAKOS_FEATURE_VM_REGIONS |
          MAKOS_FEATURE_PROCESS_STARTUP)) !=
            (MAKOS_FEATURE_SYNC | MAKOS_FEATURE_VM_REGIONS |
             MAKOS_FEATURE_PROCESS_STARTUP))
        makos_exit(117);
    if (!makos_spawn_arguments_init(&spawn_arguments, spawn_argv, 2,
                                    spawn_envp, 1) ||
        spawn_arguments.version != MAKOS_SPAWN_ARGUMENTS_VERSION ||
        spawn_arguments.argc != 2 || spawn_arguments.envc != 1 ||
        spawn_arguments.data[spawn_arguments.argv_offsets[1]] != 'w' ||
        spawn_arguments.data[spawn_arguments.env_offsets[0]] != 'M' ||
        makos_process_spawn_path_args(spawn_argv[0], text_length(spawn_argv[0]),
                                      &spawn_arguments) != -1)
        makos_exit(126);
    makos_write("MAKOS_SDK_SPAWN_ARGS_OK version=1 argc=2 envc=1 packed=1 sandbox_denied=1\n",
                sizeof("MAKOS_SDK_SPAWN_ARGS_OK version=1 argc=2 envc=1 packed=1 sandbox_denied=1\n") - 1);
    fd = makos_open(path, sizeof(path) - 1, 0);
    count = makos_read(fd, bytes, sizeof(bytes));
    if (fd < 0 || count < 5 || bytes[0] != 'M' || bytes[1] != 'a' ||
        bytes[2] != 'k' || bytes[3] != 'O' || bytes[4] != 'S' ||
        makos_close(fd) != 1)
        makos_exit(125);
    makos_write(passed, text_length(passed));
    if (makos_channel_create(channels) != -1 ||
        makos_socket_create(MAKOS_AF_INET, MAKOS_SOCK_DGRAM,
                            MAKOS_IPPROTO_UDP) != -1 ||
        makos_package_remove("hello", sizeof("hello") - 1) != -1)
        makos_exit(122);
    makos_write("MAKOS_SANDBOX_OK pid=2 uid=1001 caps=console,graphics,sync ipc_denied=1 network_denied=1 package_denied=1\n",
                sizeof("MAKOS_SANDBOX_OK pid=2 uid=1001 caps=console,graphics,sync ipc_denied=1 network_denied=1 package_denied=1\n") - 1);
    mapping = (volatile uint64_t *)makos_mmap();
    if ((intptr_t)mapping == -1)
        makos_exit(121);
    mapping[0] = 0x4d414b4f534d4d55ULL;
    if (mapping[0] != 0x4d414b4f534d4d55ULL)
        makos_exit(120);
    makos_write("MAKOS_VM_OK pid=2 mmap=1 writable=1 nx=1 munmap=1 reclaimed=1\n",
                sizeof("MAKOS_VM_OK pid=2 mmap=1 writable=1 nx=1 munmap=1 reclaimed=1\n") - 1);
    event = makos_event_create(0);
    if (event < 0)
        makos_exit(116);
    mapping[1] = (uint64_t)event;
    tid = makos_thread_create(worker_thread, (void *)mapping);
    if (tid != 3)
        makos_exit(119);
    if (makos_event_wait(event) != 1 || mapping[0] != UINT64_C(0x53594e434f4b))
        makos_exit(115);
    for (unsigned spin = 0; spin < 10000 && thread_status == -1; ++spin)
        thread_status = makos_thread_join(tid);
    if (thread_status != 7)
        makos_exit(118);
    if (makos_handle_close(event) != 1 || makos_munmap((void *)mapping) != 1)
        makos_exit(114);
    large = (volatile uint8_t *)makos_mmap_range(
        5 * 4096, MAKOS_PROT_READ | MAKOS_PROT_WRITE);
    second = (volatile uint8_t *)makos_mmap_range(
        3 * 4096, MAKOS_PROT_READ | MAKOS_PROT_WRITE);
    leaked = (volatile uint8_t *)makos_mmap_range(
        2 * 4096, MAKOS_PROT_READ | MAKOS_PROT_WRITE);
    if ((intptr_t)large == -1 || (intptr_t)second == -1 ||
        (intptr_t)leaked == -1 || large == second || second == leaked ||
        large[0] != 0 || large[5 * 4096 - 1] != 0)
        makos_exit(113);
    large[0] = 0x5a;
    large[5 * 4096 - 1] = 0xa5;
    large[4096] = 'R';
    large[4097] = 'X';
    if (makos_mprotect_range((void *)(large + 4096), 2 * 4096,
                             MAKOS_PROT_READ | MAKOS_PROT_WRITE |
                                 MAKOS_PROT_EXEC) != 0 ||
        makos_mprotect_range((void *)(large + 4096), 2 * 4096,
                             MAKOS_PROT_READ | MAKOS_PROT_EXEC) != 1 ||
        makos_write((const void *)(large + 4096), 2) != 2 ||
        makos_readdir("/", 1, 0,
                      (struct makos_dirent *)(uintptr_t)(large + 4096)) != -1 ||
        makos_munmap_range((void *)large, 5 * 4096) != 1)
        makos_exit(112);
    if (makos_write((const void *)large, 1) != -1)
        makos_exit(110);
    reused = (volatile uint8_t *)makos_mmap_range(
        4096, MAKOS_PROT_READ | MAKOS_PROT_WRITE);
    if (reused != large || reused[0] != 0 ||
        makos_munmap_range((void *)reused, 2 * 4096) != 0 ||
        makos_munmap_range((void *)reused, 4096) != 1 ||
        makos_munmap_range((void *)second, 3 * 4096) != 1)
        makos_exit(111);
    leaked[0] = 0x7c;
    leaked[2 * 4096 - 1] = 0xc7;
    makos_write("MAKOS_VM_REGION_OK pid=2 regions=4 max_region_pages=16 mapped_pages=11 zero_fill=1 partial_protect=1 wx_denied=1 copyin_rx=1 copyout_rx_denied=1 unmapped_denied=1 unmap=3 exact_length=1 hole_reuse=1 leaked_for_reaper=2\n",
                sizeof("MAKOS_VM_REGION_OK pid=2 regions=4 max_region_pages=16 mapped_pages=11 zero_fill=1 partial_protect=1 wx_denied=1 copyin_rx=1 copyout_rx_denied=1 unmapped_denied=1 unmap=3 exact_length=1 hole_reuse=1 leaked_for_reaper=2\n") - 1);
    makos_write("MAKOS_THREAD_OK pid=2 tid=3 shared_cr3=1 separate_stacks=1 join=1 exit=7\n",
                sizeof("MAKOS_THREAD_OK pid=2 tid=3 shared_cr3=1 separate_stacks=1 join=1 exit=7\n") - 1);
    makos_write("MAKOS_SYNC_OK object=event signal=1 blocking_wait=1 wake=1 thread_argument=1 handle_close=1\n",
                sizeof("MAKOS_SYNC_OK object=event signal=1 blocking_wait=1 wake=1 thread_argument=1 handle_close=1\n") - 1);

    surface = makos_surface_create(260, 160);
    fill(surface, 0, 0, 260, 160, 0xff17233a);
    fill(surface, 18, 22, 224, 30, 0xff4cde9c);
    fill(surface, 18, 74, 170, 16, 0xffdce9ff);
    fill(surface, 18, 106, 210, 12, 0xff8fa9d8);
    if (makos_surface_present(surface) != 1)
        makos_exit(123);
    makos_exit(42);
}
