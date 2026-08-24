/* MakOS Files: isolated AArch64 EL0 VFS client. */
#include <stddef.h>
#include <stdint.h>

enum {
    SYS_WRITE = 0,
    SYS_YIELD = 1,
    SYS_SURFACE_CREATE = 8,
    SYS_SURFACE_FILL = 9,
    SYS_SURFACE_PRESENT = 10,
    SYS_PROCESS_SPAWN = 14,
    SYS_PROCESS_WAIT = 15,
    SYS_CLOCK_MONOTONIC = 27,
    SYS_READ_DIR = 37,
    SYS_CREATE = 43,
    SYS_UNLINK = 44,
    SYS_SURFACE_CLOSE = 58,
    SYS_SURFACE_TEXT = 59,
    SYS_SURFACE_READ_EVENT = 60,
    SYS_RENAME = 77,
};

enum { EVENT_KEY = 1, EVENT_POINTER = 2, EVENT_RESIZE = 3, EVENT_CLOSE = 4 };
enum {
    KEY_BACKSPACE = 8,
    KEY_ENTER = 10,
    KEY_UP = 0x13,
    KEY_DOWN = 0x14,
    KEY_ESCAPE = 0x1b,
};
enum { MODE_NORMAL, MODE_CREATE, MODE_RENAME, MODE_DELETE };
enum { MAX_ENTRIES = 17, NAME_BYTES = 255, PATH_BYTES = 256 };

struct surface_event {
    uint32_t kind;
    uint32_t key;
    uint32_t modifiers;
    int32_t x;
    int32_t y;
    uint32_t width;
    uint32_t height;
};

struct directory_entry {
    uint64_t inode;
    uint32_t kind;
    uint32_t name_length;
    uint8_t name[NAME_BYTES];
};

static const char directory[] = "/home/user";
static struct directory_entry entries[MAX_ENTRIES];
static size_t entry_count;
static size_t scroll_row;
static int selected = -1;
static int last_clicked = -1;
static uint64_t last_click_tick;
static uint64_t editor_pid;
static uint64_t surface;
static uint32_t surface_width = 620;
static uint32_t surface_height = 380;
static uint8_t mode;
static char input[NAME_BYTES + 1];
static size_t input_length;
static char status[96] = "Ready";

static uint64_t syscall4(uint64_t number, uint64_t first, uint64_t second,
                         uint64_t third, uint64_t fourth) {
    register uint64_t x0 __asm__("x0") = first;
    register uint64_t x1 __asm__("x1") = second;
    register uint64_t x2 __asm__("x2") = third;
    register uint64_t x3 __asm__("x3") = fourth;
    register uint64_t x8 __asm__("x8") = number;
    __asm__ volatile("svc #0" : "+r"(x0)
                     : "r"(x1), "r"(x2), "r"(x3), "r"(x8)
                     : "memory", "cc");
    return x0;
}

static size_t text_length(const char *text) {
    size_t length = 0;
    while (text[length])
        ++length;
    return length;
}

static void copy_bytes(void *output, const void *input_bytes, size_t length) {
    uint8_t *destination = output;
    const uint8_t *source = input_bytes;
    for (size_t index = 0; index < length; ++index)
        destination[index] = source[index];
}

static void zero_bytes(void *output, size_t length) {
    uint8_t *bytes = output;
    for (size_t index = 0; index < length; ++index)
        bytes[index] = 0;
}

static int same_bytes(const uint8_t *left, const char *right, size_t length) {
    for (size_t index = 0; index < length; ++index)
        if (left[index] != (uint8_t)right[index])
            return 0;
    return right[length] == 0;
}

static void trace(const char *text) {
    syscall4(SYS_WRITE, (uintptr_t)text, text_length(text), 0, 0);
}

static void set_status(const char *text) {
    size_t length = text_length(text);
    if (length >= sizeof(status))
        length = sizeof(status) - 1;
    zero_bytes(status, sizeof(status));
    copy_bytes(status, text, length);
}

static int valid_name(const char *name, size_t length) {
    if (!length || length > NAME_BYTES ||
        (length == 1 && name[0] == '.') ||
        (length == 2 && name[0] == '.' && name[1] == '.'))
        return 0;
    for (size_t index = 0; index < length; ++index) {
        uint8_t byte = (uint8_t)name[index];
        if (!((byte >= 'a' && byte <= 'z') ||
              (byte >= 'A' && byte <= 'Z') ||
              (byte >= '0' && byte <= '9') || byte == '.' || byte == '_' ||
              byte == '-'))
            return 0;
    }
    return 1;
}

static size_t make_path(const char *name, size_t length,
                        char output[PATH_BYTES]) {
    static const char prefix[] = "/home/user/";
    if (!valid_name(name, length) || sizeof(prefix) - 1 + length >= PATH_BYTES)
        return 0;
    copy_bytes(output, prefix, sizeof(prefix) - 1);
    copy_bytes(output + sizeof(prefix) - 1, name, length);
    output[sizeof(prefix) - 1 + length] = 0;
    return sizeof(prefix) - 1 + length;
}

static void fill(uint32_t color, uint32_t x, uint32_t y, uint32_t width,
                 uint32_t height) {
    uint32_t rectangle[4] = {x, y, width, height};
    syscall4(SYS_SURFACE_FILL, surface, color, (uintptr_t)rectangle, 0);
}

static void draw_clipped(uint32_t x, uint32_t y, uint32_t right,
                         const char *text, size_t length) {
    if (x >= right)
        return;
    size_t capacity = (right - x) / 8;
    if (length > capacity)
        length = capacity;
    uint64_t point = ((uint64_t)x << 32) | y;
    syscall4(SYS_SURFACE_TEXT, surface, point, (uintptr_t)text, length);
}

static void panel(uint32_t x, uint32_t y, uint32_t width, uint32_t height,
                  int pressed) {
    fill(0xffc0c0c0, x, y, width, height);
    fill(pressed ? 0xff808080 : 0xffffffff, x, y, width, 2);
    fill(pressed ? 0xff808080 : 0xffffffff, x, y, 2, height);
    fill(pressed ? 0xffffffff : 0xff000000, x, y + height - 2, width, 2);
    fill(pressed ? 0xffffffff : 0xff000000, x + width - 2, y, 2, height);
}

static size_t visible_rows(void) {
    return surface_height > 118 ? (surface_height - 118) / 25 : 1;
}

static void button(uint32_t x, uint32_t width, const char *label) {
    panel(x, 8, width, 28, 0);
    draw_clipped(x + 7, 17, x + width - 5, label, text_length(label));
}

static void render(void) {
    fill(0xffc0c0c0, 0, 0, surface_width, surface_height);
    button(8, 48, "NEW");
    button(60, 64, "RENAME");
    button(128, 60, "DELETE");
    button(192, 48, "OPEN");
    button(244, 64, "REFRESH");
    fill(0xffffffff, 8, 42, surface_width > 16 ? surface_width - 16 : 1, 22);
    draw_clipped(14, 49, surface_width > 12 ? surface_width - 12 : 14,
                 directory, sizeof(directory) - 1);

    uint32_t list_top = 70;
    uint32_t list_bottom = surface_height > 36 ? surface_height - 36 : list_top + 1;
    fill(0xffffffff, 8, list_top, surface_width > 34 ? surface_width - 34 : 1,
         list_bottom > list_top ? list_bottom - list_top : 1);
    size_t rows = visible_rows();
    for (size_t row = 0; row < rows && scroll_row + row < entry_count; ++row) {
        size_t index = scroll_row + row;
        uint32_t y = list_top + (uint32_t)row * 25;
        if ((int)index == selected)
            fill(0xff000080, 10, y + 1,
                 surface_width > 38 ? surface_width - 38 : 1, 23);
        fill(0xffffd75f, 16, y + 6, 14, 12);
        draw_clipped(38, y + 8, surface_width > 34 ? surface_width - 34 : 38,
                     (const char *)entries[index].name,
                     entries[index].name_length);
    }
    panel(surface_width > 30 ? surface_width - 28 : 0, list_top, 20, 24, 0);
    draw_clipped(surface_width > 25 ? surface_width - 23 : 0, list_top + 8,
                 surface_width > 10 ? surface_width - 10 : surface_width, "^", 1);
    panel(surface_width > 30 ? surface_width - 28 : 0,
          list_bottom > 24 ? list_bottom - 24 : list_top, 20, 24, 0);
    draw_clipped(surface_width > 25 ? surface_width - 23 : 0,
                 list_bottom > 16 ? list_bottom - 16 : list_top,
                 surface_width > 10 ? surface_width - 10 : surface_width, "v", 1);

    fill(0xffc0c0c0, 0, surface_height > 30 ? surface_height - 30 : 0,
         surface_width, 30);
    draw_clipped(10, surface_height > 20 ? surface_height - 20 : 0,
                 surface_width > 8 ? surface_width - 8 : 10, status,
                 text_length(status));

    if (mode == MODE_CREATE || mode == MODE_RENAME) {
        uint32_t box_width = surface_width > 100 ? surface_width - 100 : 220;
        uint32_t box_x = (surface_width - box_width) / 2;
        uint32_t box_y = surface_height > 120 ? surface_height / 2 - 48 : 8;
        panel(box_x, box_y, box_width, 96, 0);
        const char *title = mode == MODE_CREATE ? "CREATE FILE" : "RENAME FILE";
        draw_clipped(box_x + 12, box_y + 14, box_x + box_width - 12, title,
                     text_length(title));
        fill(0xffffffff, box_x + 12, box_y + 38, box_width - 24, 26);
        draw_clipped(box_x + 17, box_y + 47, box_x + box_width - 17, input,
                     input_length);
        draw_clipped(box_x + 12, box_y + 76, box_x + box_width - 12,
                     "ENTER=OK  ESC=CANCEL", 19);
    } else if (mode == MODE_DELETE && selected >= 0 && (size_t)selected < entry_count) {
        uint32_t box_width = surface_width > 100 ? surface_width - 100 : 220;
        uint32_t box_x = (surface_width - box_width) / 2;
        uint32_t box_y = surface_height > 130 ? surface_height / 2 - 55 : 8;
        panel(box_x, box_y, box_width, 110, 0);
        draw_clipped(box_x + 12, box_y + 14, box_x + box_width - 12,
                     "DELETE THIS FILE?", 17);
        draw_clipped(box_x + 12, box_y + 34, box_x + box_width - 12,
                     (const char *)entries[selected].name,
                     entries[selected].name_length);
        panel(box_x + 18, box_y + 66, 70, 28, 0);
        draw_clipped(box_x + 36, box_y + 75, box_x + 82, "YES", 3);
        panel(box_x + box_width - 88, box_y + 66, 70, 28, 0);
        draw_clipped(box_x + box_width - 68, box_y + 75,
                     box_x + box_width - 24, "NO", 2);
    }
    syscall4(SYS_SURFACE_PRESENT, surface, 0, 0, 0);
}

static void refresh(void) {
    entry_count = 0;
    while (entry_count < MAX_ENTRIES &&
           syscall4(SYS_READ_DIR, (uintptr_t)directory, sizeof(directory) - 1,
                    entry_count, (uintptr_t)&entries[entry_count]) == 1)
        ++entry_count;
    if (selected >= (int)entry_count)
        selected = entry_count ? (int)entry_count - 1 : -1;
    size_t rows = visible_rows();
    if (scroll_row + rows > entry_count)
        scroll_row = entry_count > rows ? entry_count - rows : 0;
    set_status("Refreshed /home/user");
}

static int entry_named(const char *name) {
    size_t length = text_length(name);
    for (size_t index = 0; index < entry_count; ++index)
        if (entries[index].name_length == length &&
            same_bytes(entries[index].name, name, length))
            return 1;
    return 0;
}

static void vfs_self_test(void) {
    char first[PATH_BYTES], second[PATH_BYTES];
    const char *first_name = ".files-test-a";
    const char *second_name = ".files-test-b";
    size_t first_length = make_path(first_name, text_length(first_name), first);
    size_t second_length = make_path(second_name, text_length(second_name), second);
    syscall4(SYS_UNLINK, (uintptr_t)first, first_length, 0, 0);
    syscall4(SYS_UNLINK, (uintptr_t)second, second_length, 0, 0);
    if (syscall4(SYS_CREATE, (uintptr_t)first, first_length, 0, 0) != 1 ||
        syscall4(SYS_RENAME, (uintptr_t)first, first_length,
                 (uintptr_t)second, second_length) != 1) {
        trace("MAKOS_AARCH64_FILES_VFS_ERROR stage=create-or-rename\n");
        return;
    }
    refresh();
    if (!entry_named(second_name) ||
        syscall4(SYS_UNLINK, (uintptr_t)second, second_length, 0, 0) != 1) {
        trace("MAKOS_AARCH64_FILES_VFS_ERROR stage=list-or-delete\n");
        return;
    }
    refresh();
    if (entry_named(first_name) || entry_named(second_name)) {
        trace("MAKOS_AARCH64_FILES_VFS_ERROR stage=cleanup\n");
        return;
    }
    trace("MAKOS_AARCH64_FILES_VFS_OK list=real create=1 rename=1 delete=1 cleanup=1\n");
}

static void selected_path(char path[PATH_BYTES], size_t *length) {
    *length = 0;
    if (selected < 0 || (size_t)selected >= entry_count)
        return;
    *length = make_path((const char *)entries[selected].name,
                        entries[selected].name_length, path);
}

static void open_selected(void) {
    char path[PATH_BYTES];
    size_t length;
    selected_path(path, &length);
    if (!length) {
        set_status("Select a file first");
        return;
    }
    uint64_t pid = syscall4(SYS_PROCESS_SPAWN, 2, (uintptr_t)path, length, 0);
    if (pid == UINT64_MAX) {
        set_status("Text Edit launch failed or already open");
        return;
    }
    editor_pid = pid;
    set_status("Opened in Text Edit");
}

static void begin_input(uint8_t next_mode) {
    if (next_mode == MODE_RENAME && selected < 0) {
        set_status("Select a file to rename");
        return;
    }
    mode = next_mode;
    input_length = 0;
    zero_bytes(input, sizeof(input));
    if (mode == MODE_RENAME) {
        input_length = entries[selected].name_length;
        copy_bytes(input, entries[selected].name, input_length);
        input[input_length] = 0;
    }
}

static void finish_input(void) {
    if (!valid_name(input, input_length)) {
        set_status("Invalid name: use letters, digits, . _ -");
        return;
    }
    char target[PATH_BYTES];
    size_t target_length = make_path(input, input_length, target);
    int ok = 0;
    uint8_t operation = mode;
    if (operation == MODE_CREATE) {
        ok = syscall4(SYS_CREATE, (uintptr_t)target, target_length, 0, 0) == 1;
    } else {
        char source[PATH_BYTES];
        size_t source_length;
        selected_path(source, &source_length);
        ok = source_length &&
             syscall4(SYS_RENAME, (uintptr_t)source, source_length,
                      (uintptr_t)target, target_length) == 1;
    }
    mode = MODE_NORMAL;
    refresh();
    if (operation == MODE_CREATE) {
        set_status(ok ? "File created" : "Create failed: name may exist");
        if (ok)
            trace("MAKOS_AARCH64_FILES_CREATE_OK\n");
    } else {
        set_status(ok ? "File renamed" : "Rename failed");
        if (ok)
            trace("MAKOS_AARCH64_FILES_RENAME_OK\n");
    }
}

static void confirm_delete(void) {
    char path[PATH_BYTES];
    size_t length;
    selected_path(path, &length);
    int ok = length && syscall4(SYS_UNLINK, (uintptr_t)path, length, 0, 0) == 1;
    mode = MODE_NORMAL;
    refresh();
    set_status(ok ? "File deleted" : "Delete failed");
    if (ok)
        trace("MAKOS_AARCH64_FILES_DELETE_OK confirmation=explicit\n");
}

static void handle_key(uint32_t key) {
    if (mode == MODE_CREATE || mode == MODE_RENAME) {
        if (key == KEY_ESCAPE) {
            mode = MODE_NORMAL;
            set_status("Cancelled");
        } else if (key == KEY_BACKSPACE && input_length) {
            input[--input_length] = 0;
        } else if (key == KEY_ENTER) {
            uint8_t prior = mode;
            finish_input();
            if (prior == MODE_CREATE)
                trace("MAKOS_AARCH64_FILES_CREATE_ATTEMPT input=keyboard\n");
        } else if (key >= 0x20 && key <= 0x7e && input_length < NAME_BYTES) {
            input[input_length++] = (char)key;
            input[input_length] = 0;
        }
    } else if (mode == MODE_DELETE) {
        if (key == 'y' || key == 'Y')
            confirm_delete();
        else if (key == 'n' || key == 'N' || key == KEY_ESCAPE) {
            mode = MODE_NORMAL;
            set_status("Delete cancelled");
        }
    } else if (key == KEY_UP && selected > 0) {
        --selected;
        if ((size_t)selected < scroll_row)
            scroll_row = (size_t)selected;
    } else if (key == KEY_DOWN && selected + 1 < (int)entry_count) {
        ++selected;
        if ((size_t)selected >= scroll_row + visible_rows())
            scroll_row = (size_t)selected - visible_rows() + 1;
    } else if (key == KEY_ENTER) {
        open_selected();
    }
}

static int hit(int32_t x, int32_t y, uint32_t left, uint32_t top,
               uint32_t width, uint32_t height) {
    return x >= (int32_t)left && x < (int32_t)(left + width) &&
           y >= (int32_t)top && y < (int32_t)(top + height);
}

static void handle_pointer(int32_t x, int32_t y) {
    if (mode == MODE_DELETE) {
        uint32_t box_width = surface_width > 100 ? surface_width - 100 : 220;
        uint32_t box_x = (surface_width - box_width) / 2;
        uint32_t box_y = surface_height > 130 ? surface_height / 2 - 55 : 8;
        if (hit(x, y, box_x + 18, box_y + 66, 70, 28))
            confirm_delete();
        else if (hit(x, y, box_x + box_width - 88, box_y + 66, 70, 28)) {
            mode = MODE_NORMAL;
            set_status("Delete cancelled");
        }
        return;
    }
    if (mode != MODE_NORMAL)
        return;
    if (hit(x, y, 8, 8, 48, 28))
        begin_input(MODE_CREATE);
    else if (hit(x, y, 60, 8, 64, 28))
        begin_input(MODE_RENAME);
    else if (hit(x, y, 128, 8, 60, 28)) {
        if (selected >= 0)
            mode = MODE_DELETE;
        else
            set_status("Select a file to delete");
    } else if (hit(x, y, 192, 8, 48, 28))
        open_selected();
    else if (hit(x, y, 244, 8, 64, 28))
        refresh();
    else {
        uint32_t list_top = 70;
        uint32_t list_bottom = surface_height > 36 ? surface_height - 36 : list_top + 1;
        if (hit(x, y, surface_width > 30 ? surface_width - 28 : 0, list_top,
                20, 24) && scroll_row)
            --scroll_row;
        else if (hit(x, y, surface_width > 30 ? surface_width - 28 : 0,
                     list_bottom > 24 ? list_bottom - 24 : list_top, 20, 24) &&
                 scroll_row + visible_rows() < entry_count)
            ++scroll_row;
        else if (x >= 8 && x < (int32_t)surface_width - 26 &&
                 y >= (int32_t)list_top && y < (int32_t)list_bottom) {
            size_t row = (size_t)(y - (int32_t)list_top) / 25;
            size_t index = scroll_row + row;
            if (index < entry_count) {
                uint64_t now = syscall4(SYS_CLOCK_MONOTONIC, 0, 0, 0, 0);
                int double_click = last_clicked == (int)index &&
                                   now - last_click_tick <= 50;
                selected = (int)index;
                last_clicked = (int)index;
                last_click_tick = now;
                set_status(double_click ? "Opening selected file" : "Selected");
                if (double_click)
                    open_selected();
            }
        }
    }
}

__attribute__((noreturn)) void _start(void) {
    trace("MAKOS_AARCH64_FILES_ENTRY_OK el=0 process=isolated\n");
    surface = syscall4(SYS_SURFACE_CREATE, surface_width, surface_height, 6, 0);
    if (!surface || surface == UINT64_MAX)
        for (;;)
            __asm__ volatile("wfe");
    refresh();
    vfs_self_test();
    refresh();
    render();
    trace("MAKOS_AARCH64_FILES_OK surface=owned vfs=real list=1 scroll=1 select=1 double_click=text-edit mutations=verified delete_ui=confirmation-required resize=1 reopen=start-menu\n");
    for (;;) {
        if (editor_pid && syscall4(SYS_PROCESS_WAIT, editor_pid, 0, 0, 0) != UINT64_MAX)
            editor_pid = 0;
        struct surface_event event;
        if (syscall4(SYS_SURFACE_READ_EVENT, surface, (uintptr_t)&event,
                     sizeof(event), 0) != sizeof(event)) {
            syscall4(SYS_YIELD, 0, 0, 0, 0);
            continue;
        }
        int redraw = 0;
        if (event.kind == EVENT_CLOSE) {
            syscall4(SYS_SURFACE_CLOSE, surface, 0, 0, 0);
            trace("MAKOS_AARCH64_FILES_CLOSE_OK background=1 reopen=start-menu state=retained\n");
            continue;
        }
        if (event.kind == EVENT_RESIZE) {
            surface_width = event.width < 320 ? 320 : event.width;
            surface_height = event.height < 220 ? 220 : event.height;
            refresh();
            redraw = 1;
        } else if (event.kind == EVENT_POINTER && event.key) {
            handle_pointer(event.x, event.y);
            redraw = 1;
        } else if (event.kind == EVENT_KEY) {
            handle_key(event.key);
            redraw = 1;
        }
        /* Hover-only pointer motion changes no Files state. Cursor owns a
         * virtio-GPU plane, so repainting here only adds latency and CPU. */
        if (redraw)
            render();
    }
}
