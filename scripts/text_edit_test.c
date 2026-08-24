#define TEXTEDIT_TEST 1
#include "../user/aarch64_textedit.c"

#include <assert.h>
#include <stdio.h>

static void test_paths(void) {
    char path[PATH_CAPACITY];
    size_t length = 0;
    assert(canonical_path("note.txt", path, &length));
    assert(length == sizeof("/home/user/note.txt") - 1);
    assert(path[length] == 0);
    assert(canonical_path("/home/user/a-b_1.txt", path, &length));
    assert(!canonical_path("", path, &length));
    assert(!canonical_path("/etc/passwd", path, &length));
    assert(!canonical_path("../note.txt", path, &length));
    assert(!canonical_path("sub/note.txt", path, &length));
    assert(!canonical_path(".", path, &length));
    assert(!canonical_path("..", path, &length));
}

static void test_editing(void) {
    struct editor editor;
    editor_reset(&editor);
    editor_insert(&editor, 'a');
    editor_insert(&editor, 'b');
    editor_insert(&editor, '\n');
    editor_insert(&editor, 'c');
    assert(editor.length == 4 && editor.cursor == 4 && editor.dirty);
    assert(cursor_line(&editor) == 1 && cursor_column(&editor) == 1);
    editor_vertical(&editor, -1);
    assert(editor.cursor == 1); /* preferred column 1 on previous line */
    editor_left(&editor);
    editor_right(&editor);
    editor_delete(&editor);
    assert(editor.length == 3 && editor.text[0] == 'a' && editor.text[1] == '\n');
    editor_backspace(&editor);
    assert(editor.length == 2 && editor.cursor == 0 && editor.text[0] == '\n');
    editor_end(&editor);
    assert(editor.cursor == 0);
    editor_vertical(&editor, 1);
    assert(editor.cursor == 1);
    editor_home(&editor);
    assert(editor.cursor == 1);
}

static void test_capacity(void) {
    struct editor editor;
    editor_reset(&editor);
    for (size_t index = 0; index < TEXT_CAPACITY; ++index)
        editor_insert(&editor, 'x');
    editor_insert(&editor, 'y');
    assert(editor.length == TEXT_CAPACITY);
    assert(editor.status == 2);
}

static void test_selection_and_paste(void) {
    struct editor editor;
    editor_reset(&editor);
    assert(editor_insert_bytes(&editor, (const uint8_t *)"hello world", 11) == 11);
    editor.selection_anchor = 6;
    editor.cursor = 11;
    size_t start = 0;
    size_t end = 0;
    assert(editor_selection(&editor, &start, &end));
    assert(start == 6 && end == 11);
    assert(editor_insert_bytes(&editor, (const uint8_t *)"MakOS", 5) == 5);
    assert(editor.length == 11);
    assert(!editor_selection(&editor, &start, &end));
    assert(editor.text[6] == 'M' && editor.text[10] == 'S');
    editor.selection_anchor = 0;
    editor.cursor = 5;
    assert(editor_delete_selection(&editor));
    assert(editor.length == 6 && editor.text[0] == ' ');
}

static void test_load_save_dirty_contract(void) {
    struct editor editor;
    static const char path[] = "/home/user/note.txt";
    bytes_copy(fake_file, "persisted\n", 10);
    fake_file_length = 10;
    fake_read_success = 1;
    assert(editor_load(&editor, path, sizeof(path) - 1));
    assert(editor.length == 10 && !editor.dirty);
    editor_end(&editor);
    editor_insert(&editor, '!');
    fake_write_success = 0;
    assert(!editor_save(&editor, path, sizeof(path) - 1));
    assert(editor.dirty && editor.status == 3);
    fake_write_success = 1;
    assert(editor_save(&editor, path, sizeof(path) - 1));
    assert(!editor.dirty && editor.status == 1);
    assert(fake_file_length == editor.length);
}

int main(void) {
    test_paths();
    test_editing();
    test_capacity();
    test_selection_and_paste();
    test_load_save_dirty_contract();
    puts("text edit tests passed");
    return 0;
}
