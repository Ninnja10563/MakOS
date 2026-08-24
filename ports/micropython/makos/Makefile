include ../../py/mkenv.mk

QSTR_DEFS = qstrdefsport.h
MICROPY_ROM_TEXT_COMPRESSION = 0

include $(TOP)/py/py.mk

CC = $(MAKOS_CC)
CPP = $(CC) -E
LD = $(MAKOS_LD)

INC += -I.
INC += -Iinclude
INC += -I$(TOP)
INC += -I$(BUILD)

CFLAGS += $(INC)
CFLAGS += -target aarch64-unknown-none-elf
CFLAGS += -std=gnu17 -ffreestanding -fno-builtin -fno-stack-protector
CFLAGS += -fno-pic -fno-unwind-tables -fno-asynchronous-unwind-tables
CFLAGS += -mgeneral-regs-only -Os -DNDEBUG
CFLAGS += -fdata-sections -ffunction-sections
CFLAGS += -Wall -Wextra -Wno-unused-parameter -Wno-sign-compare

LDFLAGS += -flavor gnu --build-id=none -z max-page-size=4096
LDFLAGS += --gc-sections -T linker.ld

SRC_C = \
	main.c \
	runtime.c \
	shared/libc/string0.c \
	shared/libc/printf.c \
	shared/runtime/gchelper_native.c

SRC_S = gchelper_aarch64.S

OBJ += $(PY_CORE_O)
OBJ += $(addprefix $(BUILD)/, $(SRC_C:.c=.o))
OBJ += $(addprefix $(BUILD)/, $(SRC_S:.S=.o))

all: $(BUILD)/micropython-makos.elf

$(BUILD)/micropython-makos.elf: $(OBJ)
	$(ECHO) "LINK $@"
	$(Q)$(LD) $(LDFLAGS) -o $@ $^

include $(TOP)/py/mkrules.mk
