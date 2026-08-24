/* MakOS Text Edit: freestanding AArch64 EL0 text editor.
 *
 * Storage uses MakOS VFS syscalls. Rendering uses only process-owned surface
 * fill/present calls, so no kernel pointer to this buffer is retained. The
 * editor never accepts paths outside /home/user and never clears dirty state
 * unless a complete write and close succeed.
 */
#include <stddef.h>
#include <stdint.h>

enum {
    SYS_YIELD = 1,
    SYS_READ_KEY = 6,
    SYS_SURFACE_CREATE = 8,
    SYS_SURFACE_FILL = 9,
    SYS_SURFACE_PRESENT = 10,
    SYS_OPEN = 11,
    SYS_READ = 12,
    SYS_CLOSE = 13,
    SYS_FILE_WRITE = 17,
    SYS_CREATE = 43,
    SYS_SURFACE_CLOSE = 58,
    SYS_CLIPBOARD_WRITE = 110,
    SYS_CLIPBOARD_READ = 111,
#ifdef TEXTEDIT_EVENT_INPUT
    SYS_SURFACE_READ_EVENT = 60,
#endif
};

struct textedit_surface_event {
    uint32_t kind;
    uint32_t key;
    uint32_t modifiers;
    int32_t x;
    int32_t y;
    uint32_t width;
    uint32_t height;
};

enum {
    TEXT_CAPACITY = 2048, /* Must match MakFS MAX_FILE_BYTES. */
    PATH_CAPACITY = 64,
    SURFACE_WIDTH = 680,
    SURFACE_HEIGHT = 420,
    VIEW_X = 8,
    VIEW_Y = 30,
    VIEW_WIDTH = 664,
    VIEW_HEIGHT = 364,
    CELL_WIDTH = 6,
    CELL_HEIGHT = 9,
    VIEW_COLUMNS = VIEW_WIDTH / CELL_WIDTH,
    VIEW_ROWS = VIEW_HEIGHT / CELL_HEIGHT,
    KEY_LEFT = 0x11,
    KEY_RIGHT = 0x12,
    KEY_UP = 0x13,
    KEY_DOWN = 0x14,
    KEY_HOME = 0x15,
    KEY_END = 0x16,
    KEY_DELETE = 0x17,
    KEY_F2_SAVE = 0x82,
    KEY_SHORTCUT_SAVE = 0x83,
    KEY_SELECT_ALL = 0x84,
    KEY_COPY = 0x85,
    KEY_CUT = 0x86,
    KEY_PASTE = 0x87,
    KEY_ESCAPE = 0x1b,
};

struct editor {
    uint8_t text[TEXT_CAPACITY];
    size_t length;
    size_t cursor;
    size_t selection_anchor;
    size_t preferred_column;
    size_t top_line;
    size_t left_column;
    uint8_t dirty;
    uint8_t drag_selecting;
    uint8_t status; /* 0 ready, 1 saved, 2 full, 3 I/O error, 4 close refused */
};

static size_t byte_length(const char *text) {
    size_t count = 0;
    while (text[count])
        ++count;
    return count;
}

static void bytes_zero(void *destination, size_t count) {
    uint8_t *bytes = destination;
    for (size_t index = 0; index < count; ++index)
        bytes[index] = 0;
}

static void bytes_copy(void *destination, const void *source, size_t count) {
    uint8_t *output = destination;
    const uint8_t *input = source;
    for (size_t index = 0; index < count; ++index)
        output[index] = input[index];
}

/* Accept one plain MakFS name, or canonical /home/user/NAME. */
static int canonical_path(const char *input, char output[PATH_CAPACITY],
                          size_t *output_length) {
    static const char prefix[] = "/home/user/";
    size_t input_length = byte_length(input);
    size_t start = 0;
    if (input_length >= sizeof(prefix) - 1) {
        size_t index = 0;
        while (index < sizeof(prefix) - 1 && input[index] == prefix[index])
            ++index;
        if (index == sizeof(prefix) - 1)
            start = index;
        else if (input[0] == '/')
            return 0;
    } else if (input_length && input[0] == '/') {
        return 0;
    }
    size_t name_length = input_length - start;
    if (!name_length || name_length > 32 ||
        sizeof(prefix) - 1 + name_length >= PATH_CAPACITY)
        return 0;
    if ((name_length == 1 && input[start] == '.') ||
        (name_length == 2 && input[start] == '.' && input[start + 1] == '.'))
        return 0;
    for (size_t index = start; index < input_length; ++index) {
        uint8_t byte = (uint8_t)input[index];
        if (byte == '/' || byte == '\\' || byte < 0x20 || byte >= 0x7f)
            return 0;
    }
    bytes_copy(output, prefix, sizeof(prefix) - 1);
    bytes_copy(output + sizeof(prefix) - 1, input + start, name_length);
    *output_length = sizeof(prefix) - 1 + name_length;
    output[*output_length] = 0;
    return 1;
}

static void editor_reset(struct editor *editor) {
    bytes_zero(editor, sizeof(*editor));
}

static size_t line_start(const struct editor *editor, size_t position) {
    position = position > editor->length ? editor->length : position;
    while (position && editor->text[position - 1] != '\n')
        --position;
    return position;
}

static size_t line_end(const struct editor *editor, size_t position) {
    position = position > editor->length ? editor->length : position;
    while (position < editor->length && editor->text[position] != '\n')
        ++position;
    return position;
}

static size_t cursor_line(const struct editor *editor) {
    size_t line = 0;
    for (size_t index = 0; index < editor->cursor; ++index)
        if (editor->text[index] == '\n')
            ++line;
    return line;
}

static size_t cursor_column(const struct editor *editor) {
    return editor->cursor - line_start(editor, editor->cursor);
}

static int editor_selection(const struct editor *editor, size_t *start,
                            size_t *end) {
    if (editor->selection_anchor == editor->cursor)
        return 0;
    if (editor->selection_anchor < editor->cursor) {
        *start = editor->selection_anchor;
        *end = editor->cursor;
    } else {
        *start = editor->cursor;
        *end = editor->selection_anchor;
    }
    return 1;
}

static void editor_clear_selection(struct editor *editor) {
    editor->selection_anchor = editor->cursor;
    editor->drag_selecting = 0;
}

static int editor_delete_selection(struct editor *editor) {
    size_t start = 0;
    size_t end = 0;
    if (!editor_selection(editor, &start, &end))
        return 0;
    size_t count = end - start;
    for (size_t index = start; index + count < editor->length; ++index)
        editor->text[index] = editor->text[index + count];
    editor->length -= count;
    bytes_zero(editor->text + editor->length, count);
    editor->cursor = start;
    editor->selection_anchor = start;
    editor->preferred_column = cursor_column(editor);
    editor->dirty = 1;
    editor->status = 0;
    return 1;
}

static void editor_insert(struct editor *editor, uint8_t byte) {
    if (byte == '\r')
        byte = '\n';
    if (byte != '\n' && byte != '\t' && (byte < 0x20 || byte > 0x7e))
        return;
    editor_delete_selection(editor);
    if (editor->length == TEXT_CAPACITY) {
        editor->status = 2;
        return;
    }
    for (size_t index = editor->length; index > editor->cursor; --index)
        editor->text[index] = editor->text[index - 1];
    editor->text[editor->cursor++] = byte;
    ++editor->length;
    editor->preferred_column = cursor_column(editor);
    editor_clear_selection(editor);
    editor->dirty = 1;
    editor->status = 0;
}

static void editor_backspace(struct editor *editor) {
    if (editor_delete_selection(editor))
        return;
    if (!editor->cursor)
        return;
    --editor->cursor;
    for (size_t index = editor->cursor; index + 1 < editor->length; ++index)
        editor->text[index] = editor->text[index + 1];
    --editor->length;
    editor->text[editor->length] = 0;
    editor->preferred_column = cursor_column(editor);
    editor_clear_selection(editor);
    editor->dirty = 1;
    editor->status = 0;
}

static void editor_delete(struct editor *editor) {
    if (editor_delete_selection(editor))
        return;
    if (editor->cursor == editor->length)
        return;
    for (size_t index = editor->cursor; index + 1 < editor->length; ++index)
        editor->text[index] = editor->text[index + 1];
    --editor->length;
    editor->text[editor->length] = 0;
    editor->dirty = 1;
    editor->status = 0;
}

static void editor_left(struct editor *editor) {
    if (editor->cursor)
        --editor->cursor;
    editor->preferred_column = cursor_column(editor);
    editor_clear_selection(editor);
}

static void editor_right(struct editor *editor) {
    if (editor->cursor < editor->length)
        ++editor->cursor;
    editor->preferred_column = cursor_column(editor);
    editor_clear_selection(editor);
}

static void editor_vertical(struct editor *editor, int direction) {
    size_t current_start = line_start(editor, editor->cursor);
    if (direction < 0) {
        if (!current_start)
            return;
        size_t previous_end = current_start - 1;
        size_t previous_start = line_start(editor, previous_end);
        size_t previous_length = previous_end - previous_start;
        editor->cursor = previous_start +
                         (editor->preferred_column < previous_length
                              ? editor->preferred_column
                              : previous_length);
    } else {
        size_t current_end = line_end(editor, editor->cursor);
        if (current_end == editor->length)
            return;
        size_t next_start = current_end + 1;
        size_t next_end = line_end(editor, next_start);
        size_t next_length = next_end - next_start;
        editor->cursor = next_start +
                         (editor->preferred_column < next_length
                              ? editor->preferred_column
                              : next_length);
    }
    editor_clear_selection(editor);
}

static void editor_home(struct editor *editor) {
    editor->cursor = line_start(editor, editor->cursor);
    editor->preferred_column = 0;
    editor_clear_selection(editor);
}

static void editor_end(struct editor *editor) {
    editor->cursor = line_end(editor, editor->cursor);
    editor->preferred_column = cursor_column(editor);
    editor_clear_selection(editor);
}

static size_t editor_insert_bytes(struct editor *editor, const uint8_t *input,
                                  size_t count) {
    editor_delete_selection(editor);
    size_t available = TEXT_CAPACITY - editor->length;
    uint8_t truncated = count > available;
    if (truncated)
        count = available;
    size_t inserted = 0;
    for (size_t index = 0; index < count; ++index) {
        uint8_t byte = input[index] == '\r' ? '\n' : input[index];
        if (byte != '\n' && byte != '\t' && (byte < 0x20 || byte > 0x7e))
            continue;
        for (size_t offset = editor->length; offset > editor->cursor; --offset)
            editor->text[offset] = editor->text[offset - 1];
        editor->text[editor->cursor++] = byte;
        ++editor->length;
        ++inserted;
    }
    editor->preferred_column = cursor_column(editor);
    editor_clear_selection(editor);
    if (inserted) {
        editor->dirty = 1;
        editor->status = 0;
    }
    if (truncated)
        editor->status = 2;
    return inserted;
}

#ifndef TEXTEDIT_TEST

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

static int editor_read_event(uint64_t surface,
                             struct textedit_surface_event *event) {
#ifdef TEXTEDIT_EVENT_INPUT
    for (;;) {
        if (syscall4(SYS_SURFACE_READ_EVENT, surface, (uintptr_t)event,
                     sizeof(*event), 0) != sizeof(*event)) {
            syscall4(SYS_YIELD, 0, 0, 0, 0);
            continue;
        }
        if (event->kind == 4) {
            event->kind = 1;
            event->key = KEY_ESCAPE;
        }
        return 1;
    }
#else
    bytes_zero(event, sizeof(*event));
    event->kind = 1;
    event->key = (uint8_t)syscall4(SYS_READ_KEY, 0, 0, 0, 0);
    return event->key != 0;
#endif
}

static int failed(uint64_t value) { return value == UINT64_MAX; }

static int file_read_all(const char *path, size_t path_length, uint8_t *output,
                         size_t capacity, size_t *count) {
    uint64_t fd = syscall4(SYS_OPEN, (uintptr_t)path, path_length, 0, 0);
    if (failed(fd))
        return 0;
    uint64_t got = syscall4(SYS_READ, fd, (uintptr_t)output, capacity, 0);
    uint64_t closed = syscall4(SYS_CLOSE, fd, 0, 0, 0);
    if (failed(got) || got > capacity || closed != 1)
        return 0;
    *count = (size_t)got;
    return 1;
}

static int file_save_all(const char *path, size_t path_length,
                         const uint8_t *input, size_t count) {
    uint64_t fd = syscall4(SYS_OPEN, (uintptr_t)path, path_length, 1, 0);
    if (failed(fd)) {
        if (syscall4(SYS_CREATE, (uintptr_t)path, path_length, 0, 0) != 1)
            return 0;
        fd = syscall4(SYS_OPEN, (uintptr_t)path, path_length, 1, 0);
        if (failed(fd))
            return 0;
    }
    uint64_t written = syscall4(SYS_FILE_WRITE, fd, (uintptr_t)input, count, 0);
    uint64_t closed = syscall4(SYS_CLOSE, fd, 0, 0, 0);
    return written == count && closed == 1;
}

#else

static uint8_t fake_file[TEXT_CAPACITY];
static size_t fake_file_length;
static int fake_read_success = 1;
static int fake_write_success = 1;

static int file_read_all(const char *path, size_t path_length, uint8_t *output,
                         size_t capacity, size_t *count) {
    (void)path;
    (void)path_length;
    if (!fake_read_success || fake_file_length > capacity)
        return 0;
    bytes_copy(output, fake_file, fake_file_length);
    *count = fake_file_length;
    return 1;
}

static int file_save_all(const char *path, size_t path_length,
                         const uint8_t *input, size_t count) {
    (void)path;
    (void)path_length;
    if (!fake_write_success || count > sizeof(fake_file))
        return 0;
    bytes_zero(fake_file, sizeof(fake_file));
    bytes_copy(fake_file, input, count);
    fake_file_length = count;
    return 1;
}

#endif

static int editor_load(struct editor *editor, const char *path,
                       size_t path_length) {
    size_t count = 0;
    uint8_t loaded[TEXT_CAPACITY];
    if (!file_read_all(path, path_length, loaded, sizeof(loaded), &count)) {
        editor_reset(editor); /* Missing/new file starts empty, remains clean. */
        return 0;
    }
    editor_reset(editor);
    bytes_copy(editor->text, loaded, count);
    editor->length = count;
    return 1;
}

static int editor_save(struct editor *editor, const char *path,
                       size_t path_length) {
    if (!file_save_all(path, path_length, editor->text, editor->length)) {
        editor->status = 3;
        return 0;
    }
    editor->dirty = 0;
    editor->status = 1;
    return 1;
}

#ifndef TEXTEDIT_TEST

static struct editor textedit_editor;
static char textedit_path[PATH_CAPACITY];
static uint64_t textedit_surface;

static void fill(uint64_t surface, uint32_t color, uint32_t x, uint32_t y,
                 uint32_t width, uint32_t height) {
    uint32_t rectangle[4] = {x, y, width, height};
    syscall4(SYS_SURFACE_FILL, surface, color, (uintptr_t)rectangle, 0);
}

/* Public-domain-style 5x7 bitmap columns for printable ASCII 0x20..0x7e. */
static const uint8_t font[95][5] = {
    {0,0,0,0,0},{0,0,0x5f,0,0},{0,7,0,7,0},{0x14,0x7f,0x14,0x7f,0x14},
    {0x24,0x2a,0x7f,0x2a,0x12},{0x23,0x13,8,0x64,0x62},{0x36,0x49,0x55,0x22,0x50},{0,5,3,0,0},
    {0,0x1c,0x22,0x41,0},{0,0x41,0x22,0x1c,0},{0x14,8,0x3e,8,0x14},{8,8,0x3e,8,8},
    {0,0x50,0x30,0,0},{8,8,8,8,8},{0,0x60,0x60,0,0},{0x20,0x10,8,4,2},
    {0x3e,0x51,0x49,0x45,0x3e},{0,0x42,0x7f,0x40,0},{0x42,0x61,0x51,0x49,0x46},{0x21,0x41,0x45,0x4b,0x31},
    {0x18,0x14,0x12,0x7f,0x10},{0x27,0x45,0x45,0x45,0x39},{0x3c,0x4a,0x49,0x49,0x30},{1,0x71,9,5,3},
    {0x36,0x49,0x49,0x49,0x36},{6,0x49,0x49,0x29,0x1e},{0,0x36,0x36,0,0},{0,0x56,0x36,0,0},
    {8,0x14,0x22,0x41,0},{0x14,0x14,0x14,0x14,0x14},{0,0x41,0x22,0x14,8},{2,1,0x51,9,6},
    {0x32,0x49,0x79,0x41,0x3e},{0x7e,0x11,0x11,0x11,0x7e},{0x7f,0x49,0x49,0x49,0x36},{0x3e,0x41,0x41,0x41,0x22},
    {0x7f,0x41,0x41,0x22,0x1c},{0x7f,0x49,0x49,0x49,0x41},{0x7f,9,9,9,1},{0x3e,0x41,0x49,0x49,0x7a},
    {0x7f,8,8,8,0x7f},{0,0x41,0x7f,0x41,0},{0x20,0x40,0x41,0x3f,1},{0x7f,8,0x14,0x22,0x41},
    {0x7f,0x40,0x40,0x40,0x40},{0x7f,2,0x0c,2,0x7f},{0x7f,4,8,0x10,0x7f},{0x3e,0x41,0x41,0x41,0x3e},
    {0x7f,9,9,9,6},{0x3e,0x41,0x51,0x21,0x5e},{0x7f,9,0x19,0x29,0x46},{0x46,0x49,0x49,0x49,0x31},
    {1,1,0x7f,1,1},{0x3f,0x40,0x40,0x40,0x3f},{0x1f,0x20,0x40,0x20,0x1f},{0x3f,0x40,0x38,0x40,0x3f},
    {0x63,0x14,8,0x14,0x63},{7,8,0x70,8,7},{0x61,0x51,0x49,0x45,0x43},{0,0x7f,0x41,0x41,0},
    {2,4,8,0x10,0x20},{0,0x41,0x41,0x7f,0},{4,2,1,2,4},{0x40,0x40,0x40,0x40,0x40},
    {0,1,2,4,0},{0x20,0x54,0x54,0x54,0x78},{0x7f,0x48,0x44,0x44,0x38},{0x38,0x44,0x44,0x44,0x20},
    {0x38,0x44,0x44,0x48,0x7f},{0x38,0x54,0x54,0x54,0x18},{8,0x7e,9,1,2},{0x0c,0x52,0x52,0x52,0x3e},
    {0x7f,8,4,4,0x78},{0,0x44,0x7d,0x40,0},{0x20,0x40,0x44,0x3d,0},{0x7f,0x10,0x28,0x44,0},
    {0,0x41,0x7f,0x40,0},{0x7c,4,0x18,4,0x78},{0x7c,8,4,4,0x78},{0x38,0x44,0x44,0x44,0x38},
    {0x7c,0x14,0x14,0x14,8},{8,0x14,0x14,0x18,0x7c},{0x7c,8,4,4,8},{0x48,0x54,0x54,0x54,0x20},
    {4,0x3f,0x44,0x40,0x20},{0x3c,0x40,0x40,0x20,0x7c},{0x1c,0x20,0x40,0x20,0x1c},{0x3c,0x40,0x30,0x40,0x3c},
    {0x44,0x28,0x10,0x28,0x44},{0x0c,0x50,0x50,0x50,0x3c},{0x44,0x64,0x54,0x4c,0x44},{0,8,0x36,0x41,0},
    {0,0,0x7f,0,0},{0,0x41,0x36,8,0},{0x10,8,8,0x10,8}
};

static void draw_glyph(uint64_t surface, uint8_t byte, uint32_t x, uint32_t y,
                       uint32_t color) {
    if (byte < 0x20 || byte > 0x7e)
        byte = '?';
    const uint8_t *columns = font[byte - 0x20];
    for (uint32_t column = 0; column < 5; ++column) {
        uint8_t bits = columns[column];
        uint32_t row = 0;
        while (row < 7) {
            while (row < 7 && !(bits & (1u << row)))
                ++row;
            uint32_t start = row;
            while (row < 7 && (bits & (1u << row)))
                ++row;
            if (row > start)
                fill(surface, color, x + column, y + start, 1, row - start);
        }
    }
}

static void draw_text_clipped(uint64_t surface, const char *text, uint32_t x,
                              uint32_t y, uint32_t right, uint32_t color) {
    while (*text && x + 5 <= right) {
        draw_glyph(surface, (uint8_t)*text++, x, y, color);
        x += CELL_WIDTH;
    }
}

static void append_number(char *output, size_t *used, size_t capacity,
                          size_t value) {
    char reverse[24];
    size_t count = 0;
    do {
        reverse[count++] = (char)('0' + value % 10);
        value /= 10;
    } while (value && count < sizeof(reverse));
    while (count && *used + 1 < capacity)
        output[(*used)++] = reverse[--count];
}

static void append_text(char *output, size_t *used, size_t capacity,
                        const char *text) {
    while (*text && *used + 1 < capacity)
        output[(*used)++] = *text++;
}

static size_t offset_for_line(const struct editor *editor, size_t target_line) {
    size_t line = 0;
    for (size_t index = 0; index < editor->length; ++index) {
        if (line == target_line)
            return index;
        if (editor->text[index] == '\n')
            ++line;
    }
    return editor->length;
}

static size_t editor_offset_at(const struct editor *editor, int32_t x,
                               int32_t y) {
    int32_t local_x = x - VIEW_X;
    int32_t local_y = y - VIEW_Y;
    if (local_x < 0)
        local_x = 0;
    if (local_y < 0)
        local_y = 0;
    size_t row = (size_t)local_y / CELL_HEIGHT;
    size_t column = (size_t)local_x / CELL_WIDTH;
    if (row >= VIEW_ROWS)
        row = VIEW_ROWS - 1;
    if (column >= VIEW_COLUMNS)
        column = VIEW_COLUMNS - 1;
    size_t target_line = editor->top_line + row;
    size_t target_column = editor->left_column + column;
    size_t position = offset_for_line(editor, target_line);
    size_t end = line_end(editor, position);
    size_t visual_column = 0;
    while (position < end) {
        size_t width = editor->text[position] == '\t' ? 4 - (visual_column % 4) : 1;
        if (target_column < visual_column + width)
            return position;
        visual_column += width;
        ++position;
    }
    return end;
}

#ifndef TEXTEDIT_TEST
static size_t editor_copy(struct editor *editor) {
    size_t start = 0;
    size_t end = 0;
    if (!editor_selection(editor, &start, &end))
        return 0;
    uint64_t copied = syscall4(SYS_CLIPBOARD_WRITE,
                               (uintptr_t)(editor->text + start), end - start,
                               0, 0);
    return copied == end - start ? (size_t)copied : 0;
}

static size_t editor_paste(struct editor *editor) {
    uint8_t clipboard[TEXT_CAPACITY];
    uint64_t count = syscall4(SYS_CLIPBOARD_READ, (uintptr_t)clipboard,
                              sizeof(clipboard), 0, 0);
    if (count > sizeof(clipboard))
        return 0;
    return editor_insert_bytes(editor, clipboard, (size_t)count);
}
#endif

static void adjust_view(struct editor *editor) {
    size_t line = cursor_line(editor);
    size_t column = cursor_column(editor);
    if (line < editor->top_line)
        editor->top_line = line;
    else if (line >= editor->top_line + VIEW_ROWS)
        editor->top_line = line - VIEW_ROWS + 1;
    if (column < editor->left_column)
        editor->left_column = column;
    else if (column >= editor->left_column + VIEW_COLUMNS)
        editor->left_column = column - VIEW_COLUMNS + 1;
}

static void render(uint64_t surface, const struct editor *editor,
                   const char *display_name) {
    fill(surface, 0xffc0c0c0, 0, 0, SURFACE_WIDTH, SURFACE_HEIGHT);
    fill(surface, 0xffffffff, VIEW_X, VIEW_Y, VIEW_WIDTH, VIEW_HEIGHT);
    draw_text_clipped(surface, "Text Edit - ", 8, 10, SURFACE_WIDTH - 8,
                      0xff000000);
    draw_text_clipped(surface, display_name, 80, 10, SURFACE_WIDTH - 92,
                      0xff000000);
    /* Raised Save button. Ctrl-S works even when macOS function keys are media keys. */
    fill(surface, 0xffc0c0c0, SURFACE_WIDTH - 82, 4, 74, 22);
    fill(surface, 0xffffffff, SURFACE_WIDTH - 82, 4, 74, 1);
    fill(surface, 0xffffffff, SURFACE_WIDTH - 82, 4, 1, 22);
    fill(surface, 0xff000000, SURFACE_WIDTH - 82, 25, 74, 1);
    fill(surface, 0xff000000, SURFACE_WIDTH - 9, 4, 1, 22);
    draw_text_clipped(surface, "SAVE", SURFACE_WIDTH - 58, 11,
                      SURFACE_WIDTH - 12, 0xff000000);

    size_t position = offset_for_line(editor, editor->top_line);
    size_t line = editor->top_line;
    size_t column = 0;
    size_t selection_start = 0;
    size_t selection_end = 0;
    int has_selection = editor_selection(editor, &selection_start, &selection_end);
    while (position < editor->length && line < editor->top_line + VIEW_ROWS) {
        size_t byte_position = position;
        uint8_t byte = editor->text[position++];
        if (byte == '\n') {
            ++line;
            column = 0;
            continue;
        }
        size_t displayed = byte == '\t' ? 4 - (column % 4) : 1;
        while (displayed--) {
            if (column >= editor->left_column &&
                column < editor->left_column + VIEW_COLUMNS) {
                uint32_t x = VIEW_X +
                             (uint32_t)(column - editor->left_column) * CELL_WIDTH;
                uint32_t y = VIEW_Y + 1 +
                             (uint32_t)(line - editor->top_line) * CELL_HEIGHT;
                int selected = has_selection && byte_position >= selection_start &&
                               byte_position < selection_end;
                if (selected)
                    fill(surface, 0xff000080, x, y - 1, CELL_WIDTH, CELL_HEIGHT);
                if (byte != '\t')
                    draw_glyph(surface, byte, x, y,
                               selected ? 0xffffffff : 0xff000000);
            }
            ++column;
        }
    }
    size_t caret_line = cursor_line(editor);
    size_t caret_column = cursor_column(editor);
    if (!has_selection && caret_line >= editor->top_line &&
        caret_line < editor->top_line + VIEW_ROWS &&
        caret_column >= editor->left_column &&
        caret_column < editor->left_column + VIEW_COLUMNS) {
        uint32_t x = VIEW_X +
                     (uint32_t)(caret_column - editor->left_column) * CELL_WIDTH;
        uint32_t y = VIEW_Y +
                     (uint32_t)(caret_line - editor->top_line) * CELL_HEIGHT;
        fill(surface, 0xff000000, x, y, 1, CELL_HEIGHT - 1);
    }

    char status[96];
    size_t used = 0;
    append_text(status, &used, sizeof(status), "Ctrl-S/F2 Save | ");
    append_text(status, &used, sizeof(status), editor->dirty ? "Modified | " : "Saved | ");
    if (has_selection) {
        append_text(status, &used, sizeof(status), "Selected ");
        append_number(status, &used, sizeof(status), selection_end - selection_start);
        append_text(status, &used, sizeof(status), " | ");
    }
    append_number(status, &used, sizeof(status), editor->length);
    append_text(status, &used, sizeof(status), "/2048");
    if (editor->status == 2)
        append_text(status, &used, sizeof(status), " | Buffer full");
    else if (editor->status == 3)
        append_text(status, &used, sizeof(status), " | Save failed");
    else if (editor->status == 4)
        append_text(status, &used, sizeof(status), " | Ctrl-S/F2 save before close");
    status[used] = 0;
    draw_text_clipped(surface, status, 8, 404, SURFACE_WIDTH - 8, 0xff000000);
    syscall4(SYS_SURFACE_PRESENT, surface, 0, 0, 0);
}

void textedit_run(const char *file_name) {
    size_t path_length = 0;
    editor_reset(&textedit_editor);
    bytes_zero(textedit_path, sizeof(textedit_path));
    if (!canonical_path(file_name, textedit_path, &path_length)) {
        static const char invalid[] = "MAKOS_TEXT_EDIT_ERROR invalid_path=1\n";
        syscall4(0, (uintptr_t)invalid, sizeof(invalid) - 1, 0, 0);
        return;
    }
    int loaded = editor_load(&textedit_editor, textedit_path, path_length);
    if (!textedit_surface)
        textedit_surface = syscall4(SYS_SURFACE_CREATE, SURFACE_WIDTH,
                                    SURFACE_HEIGHT, 0, 0);
    if (!textedit_surface || failed(textedit_surface)) {
        static const char failed_create[] = "MAKOS_TEXT_EDIT_ERROR surface=0\n";
        syscall4(0, (uintptr_t)failed_create, sizeof(failed_create) - 1, 0, 0);
        return;
    }
    render(textedit_surface, &textedit_editor, file_name);
    if (loaded) {
        static const char opened_existing[] =
            "MAKOS_TEXT_EDIT_OPEN_OK app=native-el0 vfs=real modal=1 loaded=1\n";
        syscall4(0, (uintptr_t)opened_existing, sizeof(opened_existing) - 1, 0, 0);
    } else {
        static const char opened_new[] =
            "MAKOS_TEXT_EDIT_OPEN_OK app=native-el0 vfs=real modal=1 loaded=0\n";
        syscall4(0, (uintptr_t)opened_new, sizeof(opened_new) - 1, 0, 0);
    }
    for (;;) {
        struct textedit_surface_event event;
        if (!editor_read_event(textedit_surface, &event)) {
            /* Input is nonblocking. Never redraw or consume a full CPU while
             * idle; yield until scheduler gives us another input poll. */
            syscall4(SYS_YIELD, 0, 0, 0, 0);
            continue;
        }
        if (event.kind == 2) {
            /* No hover UI. Ignore button-up motion unless completing active
             * drag selection; hardware cursor moves independently. */
            if (!(event.key & 1) && !textedit_editor.drag_selecting)
                continue;
            size_t position = editor_offset_at(&textedit_editor, event.x, event.y);
            if (event.key & 1) {
                if (!textedit_editor.drag_selecting) {
                    textedit_editor.cursor = position;
                    textedit_editor.selection_anchor = position;
                    textedit_editor.drag_selecting = 1;
                } else {
                    textedit_editor.cursor = position;
                }
                textedit_editor.preferred_column = cursor_column(&textedit_editor);
            } else if (textedit_editor.drag_selecting) {
                textedit_editor.cursor = position;
                textedit_editor.drag_selecting = 0;
                textedit_editor.preferred_column = cursor_column(&textedit_editor);
            }
            adjust_view(&textedit_editor);
            render(textedit_surface, &textedit_editor, file_name);
            continue;
        }
        if (event.kind != 1)
            continue;
        uint8_t key = (uint8_t)event.key;
        if (!key)
            continue;
        if (key == 8)
            editor_backspace(&textedit_editor);
        else if (key == KEY_DELETE)
            editor_delete(&textedit_editor);
        else if (key == KEY_LEFT)
            editor_left(&textedit_editor);
        else if (key == KEY_RIGHT)
            editor_right(&textedit_editor);
        else if (key == KEY_UP)
            editor_vertical(&textedit_editor, -1);
        else if (key == KEY_DOWN)
            editor_vertical(&textedit_editor, 1);
        else if (key == KEY_HOME)
            editor_home(&textedit_editor);
        else if (key == KEY_END)
            editor_end(&textedit_editor);
        else if (key == KEY_SELECT_ALL) {
            textedit_editor.selection_anchor = 0;
            textedit_editor.cursor = textedit_editor.length;
            textedit_editor.drag_selecting = 0;
        } else if (key == KEY_COPY) {
            size_t copied = editor_copy(&textedit_editor);
            if (copied) {
                static const char copied_ok[] =
                    "MAKOS_CLIPBOARD_OK action=copy source=text-edit highlight=visible\n";
                syscall4(0, (uintptr_t)copied_ok, sizeof(copied_ok) - 1, 0, 0);
            }
        } else if (key == KEY_CUT) {
            size_t copied = editor_copy(&textedit_editor);
            if (copied) {
                editor_delete_selection(&textedit_editor);
                static const char cut_ok[] =
                    "MAKOS_CLIPBOARD_OK action=cut source=text-edit selection=deleted\n";
                syscall4(0, (uintptr_t)cut_ok, sizeof(cut_ok) - 1, 0, 0);
            }
        } else if (key == KEY_PASTE) {
            size_t pasted = editor_paste(&textedit_editor);
            if (pasted) {
                static const char paste_ok[] =
                    "MAKOS_CLIPBOARD_OK action=paste target=text-edit selection=replaced\n";
                syscall4(0, (uintptr_t)paste_ok, sizeof(paste_ok) - 1, 0, 0);
            }
        }
        else if (key == KEY_F2_SAVE || key == KEY_SHORTCUT_SAVE) {
            if (editor_save(&textedit_editor, textedit_path, path_length)) {
                if (key == KEY_F2_SAVE) {
                    static const char saved_f2[] =
                        "MAKOS_TEXT_EDIT_SAVE_OK key=F2 write=complete dirty=0\n";
                    syscall4(0, (uintptr_t)saved_f2, sizeof(saved_f2) - 1, 0, 0);
                } else {
                    static const char saved_shortcut[] =
                        "MAKOS_TEXT_EDIT_SAVE_OK key=Ctrl-S write=complete dirty=0\n";
                    syscall4(0, (uintptr_t)saved_shortcut,
                             sizeof(saved_shortcut) - 1, 0, 0);
                }
            }
        } else if (key == KEY_ESCAPE) {
            if (textedit_editor.dirty) {
                textedit_editor.status = 4;
                static const char refused[] =
                    "MAKOS_TEXT_EDIT_CLOSE_REFUSED dirty=1 save=Ctrl-S-or-F2\n";
                syscall4(0, (uintptr_t)refused, sizeof(refused) - 1, 0, 0);
                adjust_view(&textedit_editor);
                render(textedit_surface, &textedit_editor, file_name);
                continue;
            }
            syscall4(SYS_SURFACE_CLOSE, textedit_surface, 0, 0, 0);
            static const char closed[] =
                "MAKOS_TEXT_EDIT_CLOSE_OK key=Escape return=shell\n";
            syscall4(0, (uintptr_t)closed, sizeof(closed) - 1, 0, 0);
            return;
        }
        else if (key == '\n' || key == '\t' ||
                 (key >= 0x20 && key <= 0x7e))
            editor_insert(&textedit_editor, key);
        adjust_view(&textedit_editor);
        render(textedit_surface, &textedit_editor, file_name);
    }
}

#ifndef TEXTEDIT_EMBEDDED
__attribute__((noreturn)) void _start(void) {
    textedit_run("note.txt");
    syscall4(5, 0, 0, 0, 0);
    for (;;)
        __asm__ volatile("wfe");
}
#endif

#endif
