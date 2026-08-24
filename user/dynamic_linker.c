#include <stddef.h>
#include <stdint.h>

enum {
    SYS_WRITE = 0,
    SYS_EXIT = 5,
    PT_LOAD = 1,
    PT_DYNAMIC = 2,
    DT_NULL = 0,
    DT_NEEDED = 1,
    DT_PLTRELSZ = 2,
    DT_HASH = 4,
    DT_STRTAB = 5,
    DT_SYMTAB = 6,
    DT_RELA = 7,
    DT_RELASZ = 8,
    DT_RELAENT = 9,
    DT_STRSZ = 10,
    DT_SYMENT = 11,
    DT_PLTREL = 20,
    DT_JMPREL = 23,
    DT_RELA_TAG = 7,
    R_X86_64_64 = 1,
    R_X86_64_GLOB_DAT = 6,
    R_X86_64_JUMP_SLOT = 7,
    R_X86_64_RELATIVE = 8,
};

typedef struct {
    unsigned char ident[16];
    uint16_t type;
    uint16_t machine;
    uint32_t version;
    uint64_t entry;
    uint64_t phoff;
    uint64_t shoff;
    uint32_t flags;
    uint16_t ehsize;
    uint16_t phentsize;
    uint16_t phnum;
    uint16_t shentsize;
    uint16_t shnum;
    uint16_t shstrndx;
} Elf64_Ehdr;

typedef struct {
    uint32_t type;
    uint32_t flags;
    uint64_t offset;
    uint64_t vaddr;
    uint64_t paddr;
    uint64_t filesz;
    uint64_t memsz;
    uint64_t align;
} Elf64_Phdr;

typedef struct {
    int64_t tag;
    uint64_t value;
} Elf64_Dyn;

typedef struct {
    uint32_t name;
    unsigned char info;
    unsigned char other;
    uint16_t shndx;
    uint64_t value;
    uint64_t size;
} Elf64_Sym;

typedef struct {
    uint64_t offset;
    uint64_t info;
    int64_t addend;
} Elf64_Rela;

typedef struct {
    uintptr_t base;
    const Elf64_Ehdr *header;
    const Elf64_Dyn *dynamic;
    const char *strings;
    const Elf64_Sym *symbols;
    const uint32_t *hash;
    const Elf64_Rela *rela;
    size_t rela_count;
    const Elf64_Rela *plt_rela;
    size_t plt_rela_count;
    size_t string_bytes;
    size_t symbol_count;
    size_t needed_count;
} Image;

static uint64_t syscall3(uint64_t number, uint64_t first, uint64_t second,
                         uint64_t third) {
    register uint64_t rax __asm__("rax") = number;
    register uint64_t rdi __asm__("rdi") = first;
    register uint64_t rsi __asm__("rsi") = second;
    register uint64_t rdx __asm__("rdx") = third;
    __asm__ volatile("int $0x80"
                     : "+a"(rax)
                     : "D"(rdi), "S"(rsi), "d"(rdx)
                     : "rcx", "r8", "r9", "r10", "r11", "memory");
    return rax;
}

static __attribute__((noreturn)) void fail(uint64_t code) {
    syscall3(SYS_EXIT, code, 0, 0);
    __builtin_trap();
}

static int equal_string(const char *left, const char *right, size_t limit) {
    for (size_t index = 0; index < limit; ++index) {
        if (left[index] != right[index])
            return 0;
        if (left[index] == '\0')
            return 1;
    }
    return 0;
}

static int writable_target(const Image *image, uintptr_t address, size_t bytes) {
    const Elf64_Phdr *headers =
        (const Elf64_Phdr *)(image->base + image->header->phoff);
    uintptr_t end = address + bytes;
    if (end < address)
        return 0;
    for (size_t index = 0; index < image->header->phnum; ++index) {
        const Elf64_Phdr *header = &headers[index];
        uintptr_t start = image->base + header->vaddr;
        uintptr_t segment_end = start + header->memsz;
        if (header->type == PT_LOAD && (header->flags & 2) != 0 &&
            address >= start && end <= segment_end)
            return 1;
    }
    return 0;
}

static Image parse_image(uintptr_t base) {
    Image image = {0};
    image.base = base;
    image.header = (const Elf64_Ehdr *)base;
    if (image.header->ident[0] != 0x7f || image.header->ident[1] != 'E' ||
        image.header->ident[2] != 'L' || image.header->ident[3] != 'F' ||
        image.header->ident[4] != 2 || image.header->ident[5] != 1 ||
        image.header->machine != 62 || image.header->phentsize != sizeof(Elf64_Phdr) ||
        image.header->phnum == 0 || image.header->phnum > 16)
        fail(80);
    const Elf64_Phdr *headers =
        (const Elf64_Phdr *)(base + image.header->phoff);
    for (size_t index = 0; index < image.header->phnum; ++index) {
        if (headers[index].type == PT_DYNAMIC) {
            image.dynamic = (const Elf64_Dyn *)(base + headers[index].vaddr);
            break;
        }
    }
    if (image.dynamic == 0)
        fail(81);
    size_t rela_bytes = 0;
    size_t plt_rela_bytes = 0;
    size_t rela_entry_bytes = 0;
    uint64_t plt_kind = 0;
    for (size_t index = 0; index < 128; ++index) {
        const Elf64_Dyn entry = image.dynamic[index];
        if (entry.tag == DT_NULL)
            break;
        switch (entry.tag) {
        case DT_NEEDED:
            ++image.needed_count;
            break;
        case DT_HASH:
            image.hash = (const uint32_t *)(base + entry.value);
            break;
        case DT_STRTAB:
            image.strings = (const char *)(base + entry.value);
            break;
        case DT_SYMTAB:
            image.symbols = (const Elf64_Sym *)(base + entry.value);
            break;
        case DT_RELA:
            image.rela = (const Elf64_Rela *)(base + entry.value);
            break;
        case DT_RELASZ:
            rela_bytes = entry.value;
            break;
        case DT_RELAENT:
            rela_entry_bytes = entry.value;
            break;
        case DT_STRSZ:
            image.string_bytes = entry.value;
            break;
        case DT_SYMENT:
            if (entry.value != sizeof(Elf64_Sym))
                fail(82);
            break;
        case DT_PLTRELSZ:
            plt_rela_bytes = entry.value;
            break;
        case DT_PLTREL:
            plt_kind = entry.value;
            break;
        case DT_JMPREL:
            image.plt_rela = (const Elf64_Rela *)(base + entry.value);
            break;
        }
    }
    if (image.strings == 0 || image.symbols == 0 || image.hash == 0 ||
        image.string_bytes == 0 || image.hash[1] == 0 ||
        (rela_bytes != 0 && (image.rela == 0 || rela_entry_bytes != sizeof(Elf64_Rela))) ||
        (plt_rela_bytes != 0 &&
         (image.plt_rela == 0 || plt_kind != DT_RELA_TAG ||
          plt_rela_bytes % sizeof(Elf64_Rela) != 0)) ||
        rela_bytes % sizeof(Elf64_Rela) != 0)
        fail(83);
    image.symbol_count = image.hash[1];
    image.rela_count = rela_bytes / sizeof(Elf64_Rela);
    image.plt_rela_count = plt_rela_bytes / sizeof(Elf64_Rela);
    return image;
}

static uintptr_t resolve(const Image *library, const char *name) {
    for (size_t index = 1; index < library->symbol_count; ++index) {
        const Elf64_Sym *symbol = &library->symbols[index];
        if (symbol->shndx != 0 && symbol->name < library->string_bytes &&
            equal_string(name, library->strings + symbol->name,
                         library->string_bytes - symbol->name))
            return library->base + symbol->value;
    }
    return 0;
}

static void relocate_table(const Image *image, const Image *library,
                           const Elf64_Rela *relocations, size_t count) {
    for (size_t index = 0; index < count; ++index) {
        const Elf64_Rela relocation = relocations[index];
        uint32_t type = (uint32_t)relocation.info;
        uint32_t symbol_index = (uint32_t)(relocation.info >> 32);
        uintptr_t target = image->base + relocation.offset;
        if (!writable_target(image, target, sizeof(uintptr_t)))
            fail(84);
        uintptr_t value;
        if (type == R_X86_64_RELATIVE) {
            value = image->base + relocation.addend;
        } else if (type == R_X86_64_64 || type == R_X86_64_GLOB_DAT ||
                   type == R_X86_64_JUMP_SLOT) {
            if (symbol_index >= image->symbol_count ||
                image->symbols[symbol_index].name >= image->string_bytes)
                fail(85);
            const char *name =
                image->strings + image->symbols[symbol_index].name;
            value = resolve(library, name);
            if (value == 0)
                fail(86);
            value += relocation.addend;
        } else {
            fail(87);
        }
        *(volatile uintptr_t *)target = value;
    }
}

void _start(uintptr_t application_base, uintptr_t library_base,
            uintptr_t application_entry) {
    static const char passed[] =
        "MAKOS_DYNAMIC_LINKER_OK loader=ld-makos.so app=dynamic-app "
        "library=libmakosdemo.so needed=1 rela=1 plt=1 symbols=sysv-hash "
        "wx=1 userspace=1\n";
    Image application = parse_image(application_base);
    Image library = parse_image(library_base);
    if (application.needed_count != 1 || application_entry !=
            application_base + application.header->entry)
        fail(88);
    relocate_table(&library, &library, library.rela, library.rela_count);
    relocate_table(&library, &library, library.plt_rela,
                   library.plt_rela_count);
    relocate_table(&application, &library, application.rela,
                   application.rela_count);
    relocate_table(&application, &library, application.plt_rela,
                   application.plt_rela_count);
    syscall3(SYS_WRITE, (uintptr_t)passed, sizeof(passed) - 1, 0);
    ((void (*)(void))application_entry)();
    fail(89);
}
