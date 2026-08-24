#include <stddef.h>
#include <stdint.h>

typedef uint64_t HANDLE;
typedef int BOOL;
typedef uint32_t DWORD;

enum {
    WIN_OP_WRITE_FILE = 1,
    WIN_OP_GET_PROCESS_ID = 2,
    WIN_OP_GET_TICK_COUNT = 3,
    WIN_OP_CREATE_EVENT = 4,
    WIN_OP_SET_EVENT = 5,
    WIN_OP_WAIT_OBJECT = 6,
    WIN_OP_CLOSE_HANDLE = 7,
    WIN_OP_EXIT_PROCESS = 8,
    WIN_WAIT_OBJECT_0 = 0,
    WIN_INFINITE = 0xffffffffu,
};

static uint64_t win_call4(uint64_t operation, uint64_t first, uint64_t second,
                          uint64_t third, uint64_t fourth) {
    register uint64_t arg4 __asm__("r10") = fourth;
    __asm__ volatile("int $0x80"
                     : "+a"(operation)
                     : "D"(first), "S"(second), "d"(third), "r"(arg4)
                     : "memory", "cc");
    return operation;
}

__attribute__((ms_abi)) BOOL WriteFile(HANDLE handle, const void *bytes,
                                        DWORD length, DWORD *written) {
    return (BOOL)win_call4(WIN_OP_WRITE_FILE, handle, (uintptr_t)bytes, length,
                           (uintptr_t)written);
}

__attribute__((ms_abi)) DWORD GetCurrentProcessId(void) {
    return (DWORD)win_call4(WIN_OP_GET_PROCESS_ID, 0, 0, 0, 0);
}

__attribute__((ms_abi)) uint64_t GetTickCount64(void) {
    return win_call4(WIN_OP_GET_TICK_COUNT, 0, 0, 0, 0);
}

__attribute__((ms_abi)) HANDLE CreateEventA(void *attributes, BOOL manual_reset,
                                            BOOL initial_state,
                                            const char *name) {
    if (attributes != NULL || manual_reset || name != NULL)
        return 0;
    return win_call4(WIN_OP_CREATE_EVENT, initial_state != 0, 0, 0, 0);
}

__attribute__((ms_abi)) BOOL SetEvent(HANDLE event) {
    return (BOOL)win_call4(WIN_OP_SET_EVENT, event, 0, 0, 0);
}

__attribute__((ms_abi)) DWORD WaitForSingleObject(HANDLE event,
                                                  DWORD milliseconds) {
    return (DWORD)win_call4(WIN_OP_WAIT_OBJECT, event, milliseconds, 0, 0);
}

__attribute__((ms_abi)) BOOL CloseHandle(HANDLE handle) {
    return (BOOL)win_call4(WIN_OP_CLOSE_HANDLE, handle, 0, 0, 0);
}

__attribute__((ms_abi, noreturn)) void ExitProcess(DWORD status) {
    win_call4(WIN_OP_EXIT_PROCESS, status, 0, 0, 0);
    __builtin_unreachable();
}

__attribute__((section(".text._start"))) void _start(void) {
    static const char passed[] =
        "MAKOS_WINDOWS_APP_OK write=1 pid=1 time=1 event=1 wait=1 close=1 "
        "exit=1 ms_abi=1 pe32=1\n";
    DWORD written = 0;
    HANDLE event = CreateEventA(NULL, 0, 0, NULL);
    if (GetCurrentProcessId() != 4 || GetTickCount64() == 0 || event == 0 ||
        !SetEvent(event) || WaitForSingleObject(event, WIN_INFINITE) != WIN_WAIT_OBJECT_0 ||
        !CloseHandle(event) ||
        !WriteFile(1, passed, (DWORD)(sizeof(passed) - 1), &written) ||
        written != sizeof(passed) - 1)
        ExitProcess(93);
    ExitProcess(42);
}
