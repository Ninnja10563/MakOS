SHELL := /bin/sh

BUILD := build
KERNEL := target/x86_64-unknown-none/release/makos-kernel
LOADER := target/x86_64-unknown-uefi/release/makos-loader.efi
IMAGE := $(BUILD)/makos-x86_64.img
DATA_IMAGE := $(BUILD)/makos-data.img
X86_64_GPT_DATA_IMAGE := $(BUILD)/makos-data-x86_64-gpt-seed.img
X86_64_GPT_ESP_IMAGE := $(BUILD)/makos-x86_64-gpt-esp.img
X86_64_GPT_IMAGE := $(BUILD)/makos-x86_64-gpt.img
AARCH64_KERNEL := target/aarch64-unknown-none/release/makos-kernel
AARCH64_LOADER := target/aarch64-unknown-uefi/release/makos-loader.efi
AARCH64_IMAGE := $(BUILD)/makos-aarch64.img
AARCH64_SMP_INPUT_IMAGE := $(BUILD)/makos-aarch64-smp-input.img
AARCH64_SMP_TCP_IMAGE := $(BUILD)/makos-aarch64-smp-tcp.img
AARCH64_DATA_IMAGE := $(BUILD)/makos-data-aarch64.img
AARCH64_GPT_DATA_IMAGE := $(BUILD)/makos-data-aarch64-gpt-seed.img
AARCH64_GPT_IMAGE := $(BUILD)/makos-aarch64-gpt.img
CPYTHON_AARCH64_DATA_IMAGE := $(BUILD)/makos-cpython-3.14.7-data.img
AARCH64_FIREFOX_PACKAGE_IMAGE ?= $(BUILD)/makos-integrated-firefox-handoff149.img
SOURCE_DATA_IMAGE ?=
INTEGRATED_OUTPUT_DIR ?= $(BUILD)
INSTALL_TARGET ?=

.PHONY: all build build-aarch64 image image-x86_64-gpt image-aarch64 image-aarch64-smp-input image-aarch64-smp-tcp image-aarch64-gpt data-aarch64 cpython-aarch64 package-cpython-aarch64 integrated-data-aarch64 test-integrated-data run run-x86_64-gpt run-x86_64-installer run-aarch64 run-aarch64-gpt test test-x86_64-gpt test-x86_64-install test-aarch64 test-aarch64-smp-input-runtime test-aarch64-smp-tcp-runtime test-aarch64-smp-migration-runtime test-aarch64-smp-load-runtime test-aarch64-production-smp-runtime test-aarch64-native-smp-runtime test-aarch64-selfhost-runtime test-makfs4-guest-fsck test-aarch64-cursor-runtime test-aarch64-firefox-runtime test-aarch64-ipv6-runtime test-aarch64-package-runtime test-aarch64-gpt test-aarch64-install test-cpython-aarch64 unit check release clean

all: image

build:
	cargo build --release -p makos-kernel --target x86_64-unknown-none
	cargo build --release -p makos-loader --target x86_64-unknown-uefi

build-aarch64:
	cargo build --release -p makos-kernel --target aarch64-unknown-none
	cargo build --release -p makos-loader --target aarch64-unknown-uefi

image: build
	mkdir -p $(BUILD)/esp/EFI/BOOT
	cp $(LOADER) $(BUILD)/esp/EFI/BOOT/BOOTX64.EFI
	cp $(KERNEL) $(BUILD)/esp/KERNEL.ELF
	python3 scripts/mkfat.py $(IMAGE) \
		EFI/BOOT/BOOTX64.EFI=$(BUILD)/esp/EFI/BOOT/BOOTX64.EFI \
		KERNEL.ELF=$(BUILD)/esp/KERNEL.ELF \
		MAKOS.CFG=boot/MAKOS.CFG
	python3 scripts/check_artifacts.py $(IMAGE) $(KERNEL)
	python3 scripts/mkdata.py $(DATA_IMAGE)

image-x86_64-gpt: build
	mkdir -p $(BUILD)/esp-x86_64-gpt/EFI/BOOT
	cp $(LOADER) $(BUILD)/esp-x86_64-gpt/EFI/BOOT/BOOTX64.EFI
	cp $(KERNEL) $(BUILD)/esp-x86_64-gpt/KERNEL.ELF
	python3 scripts/mkfat.py $(X86_64_GPT_ESP_IMAGE) \
		EFI/BOOT/BOOTX64.EFI=$(BUILD)/esp-x86_64-gpt/EFI/BOOT/BOOTX64.EFI \
		KERNEL.ELF=$(BUILD)/esp-x86_64-gpt/KERNEL.ELF \
		MAKOS.CFG=boot/MAKOS.CFG
	python3 scripts/check_artifacts.py $(X86_64_GPT_ESP_IMAGE) $(KERNEL)
	python3 scripts/mkdata.py $(X86_64_GPT_DATA_IMAGE)
	python3 scripts/mkgpt.py $(X86_64_GPT_IMAGE) --esp $(X86_64_GPT_ESP_IMAGE) --data $(X86_64_GPT_DATA_IMAGE)

image-aarch64: build-aarch64
	mkdir -p $(BUILD)/esp-aarch64/EFI/BOOT
	cp $(AARCH64_LOADER) $(BUILD)/esp-aarch64/EFI/BOOT/BOOTAA64.EFI
	cp $(AARCH64_KERNEL) $(BUILD)/esp-aarch64/KERNEL.ELF
	python3 scripts/mkfat.py $(AARCH64_IMAGE) \
		EFI/BOOT/BOOTAA64.EFI=$(BUILD)/esp-aarch64/EFI/BOOT/BOOTAA64.EFI \
		KERNEL.ELF=$(BUILD)/esp-aarch64/KERNEL.ELF \
		MAKOS.CFG=boot/MAKOS.CFG
	python3 scripts/check_artifacts.py $(AARCH64_IMAGE) $(AARCH64_KERNEL)

image-aarch64-smp-input: build-aarch64
	mkdir -p $(BUILD)/esp-aarch64-smp-input/EFI/BOOT
	cp $(AARCH64_LOADER) $(BUILD)/esp-aarch64-smp-input/EFI/BOOT/BOOTAA64.EFI
	cp $(AARCH64_KERNEL) $(BUILD)/esp-aarch64-smp-input/KERNEL.ELF
	python3 scripts/mkfat.py $(AARCH64_SMP_INPUT_IMAGE) \
		EFI/BOOT/BOOTAA64.EFI=$(BUILD)/esp-aarch64-smp-input/EFI/BOOT/BOOTAA64.EFI \
		KERNEL.ELF=$(BUILD)/esp-aarch64-smp-input/KERNEL.ELF \
		MAKOS.CFG=boot/MAKOS-SMP-INPUT.CFG
	python3 scripts/check_artifacts.py $(AARCH64_SMP_INPUT_IMAGE) $(AARCH64_KERNEL)

image-aarch64-smp-tcp: build-aarch64
	mkdir -p $(BUILD)/esp-aarch64-smp-tcp/EFI/BOOT
	cp $(AARCH64_LOADER) $(BUILD)/esp-aarch64-smp-tcp/EFI/BOOT/BOOTAA64.EFI
	cp $(AARCH64_KERNEL) $(BUILD)/esp-aarch64-smp-tcp/KERNEL.ELF
	python3 scripts/mkfat.py $(AARCH64_SMP_TCP_IMAGE) \
		EFI/BOOT/BOOTAA64.EFI=$(BUILD)/esp-aarch64-smp-tcp/EFI/BOOT/BOOTAA64.EFI \
		KERNEL.ELF=$(BUILD)/esp-aarch64-smp-tcp/KERNEL.ELF \
		MAKOS.CFG=boot/MAKOS-SMP-TCP.CFG
	python3 scripts/check_artifacts.py $(AARCH64_SMP_TCP_IMAGE) $(AARCH64_KERNEL)

image-aarch64-gpt: image-aarch64
	python3 scripts/mkdata.py $(AARCH64_GPT_DATA_IMAGE)
	python3 scripts/mkgpt.py $(AARCH64_GPT_IMAGE) --esp $(AARCH64_IMAGE) --data $(AARCH64_GPT_DATA_IMAGE)

data-aarch64:
	@test -f $(AARCH64_DATA_IMAGE) || python3 scripts/mkdata.py $(AARCH64_DATA_IMAGE)

cpython-aarch64:
	./ports/cpython/build-makos.sh

package-cpython-aarch64: cpython-aarch64
	./ports/cpython/package-makos.sh $(CPYTHON_AARCH64_DATA_IMAGE)

integrated-data-aarch64:
	@test -n "$(SOURCE_DATA_IMAGE)" || { echo "set SOURCE_DATA_IMAGE to an existing MakOS data image" >&2; exit 2; }
	python3 scripts/integrate_data_image.py "$(SOURCE_DATA_IMAGE)" --output-dir "$(INTEGRATED_OUTPUT_DIR)"

test-integrated-data:
	python3 scripts/test_integrated_data.py

run: image
	./scripts/run-qemu.sh $(IMAGE)

run-x86_64-gpt:
	@test -f $(X86_64_GPT_IMAGE) || $(MAKE) image-x86_64-gpt
	./scripts/run-qemu-x86_64-gpt.sh $(X86_64_GPT_IMAGE)

run-x86_64-installer:
	@test -f $(X86_64_GPT_IMAGE) || $(MAKE) image-x86_64-gpt
	@test -n "$(INSTALL_TARGET)" || { echo "set INSTALL_TARGET to an existing blank disk image" >&2; exit 2; }
	@test -f "$(INSTALL_TARGET)" || { echo "INSTALL_TARGET not found: $(INSTALL_TARGET)" >&2; exit 2; }
	./scripts/run-qemu-x86_64-gpt.sh $(X86_64_GPT_IMAGE) "$(INSTALL_TARGET)"

run-aarch64: image-aarch64 data-aarch64
	./scripts/run-qemu-aarch64.sh $(AARCH64_IMAGE) $(AARCH64_DATA_IMAGE)

run-aarch64-gpt:
	@test -f $(AARCH64_GPT_IMAGE) || $(MAKE) image-aarch64-gpt
	./scripts/run-qemu-aarch64-gpt.sh $(AARCH64_GPT_IMAGE)

test: image unit
	python3 scripts/boot_test.py $(IMAGE) $(DATA_IMAGE)

test-x86_64-gpt: image-x86_64-gpt
	python3 scripts/boot_test.py --gpt $(X86_64_GPT_IMAGE)

test-x86_64-install: image-x86_64-gpt
	MAKOS_X86_64_GPT_IMAGE=$(X86_64_GPT_IMAGE) python3 scripts/boot_test_x86_64_install.py

test-aarch64: image-aarch64
	python3 scripts/boot_test_aarch64.py

test-aarch64-smp-input-runtime: image-aarch64-smp-input
	MAKOS_AARCH64_IMAGE=$(AARCH64_SMP_INPUT_IMAGE) python3 scripts/boot_test_aarch64_smp_input.py

test-aarch64-smp-tcp-runtime: image-aarch64-smp-tcp
	MAKOS_AARCH64_IMAGE=$(AARCH64_SMP_TCP_IMAGE) python3 scripts/boot_test_aarch64_smp_tcp.py

test-aarch64-smp-migration-runtime: image-aarch64
	MAKOS_AARCH64_IMAGE=$(AARCH64_IMAGE) python3 scripts/boot_test_aarch64_smp_migration.py

test-aarch64-smp-load-runtime: image-aarch64
	MAKOS_AARCH64_IMAGE=$(AARCH64_IMAGE) python3 scripts/boot_test_aarch64_smp_migration.py

test-aarch64-production-smp-runtime: image-aarch64
	MAKOS_AARCH64_IMAGE=$(AARCH64_IMAGE) python3 scripts/boot_test_aarch64_production_smp.py

test-aarch64-native-smp-runtime: image-aarch64
	MAKOS_AARCH64_IMAGE=$(AARCH64_IMAGE) python3 scripts/boot_test_aarch64_native_smp.py

test-aarch64-selfhost-runtime: image-aarch64
	MAKOS_AARCH64_IMAGE=$(AARCH64_IMAGE) python3 scripts/boot_test_aarch64_selfhost.py

test-makfs4-guest-fsck: image-aarch64
	python3 scripts/test_makfs4_guest_fsck.py

test-aarch64-cursor-runtime: image-aarch64
	python3 scripts/boot_test_aarch64_cursor.py

test-aarch64-firefox-runtime: image-aarch64
	@test -f "$(AARCH64_FIREFOX_PACKAGE_IMAGE)" || { echo "Firefox package image not found: $(AARCH64_FIREFOX_PACKAGE_IMAGE)" >&2; exit 2; }
	python3 scripts/verify_firefox_runtime_image.py "$(AARCH64_FIREFOX_PACKAGE_IMAGE)"
	MAKOS_AARCH64_PACKAGE_IMAGE="$(AARCH64_FIREFOX_PACKAGE_IMAGE)" \
	MAKOS_AARCH64_FIREFOX_PROBE=1 \
	MAKOS_AARCH64_FIREFOX_PROBE_SECONDS=600 \
	MAKOS_AARCH64_FIREFOX_NAVIGATE=1 \
	MAKOS_AARCH64_FIREFOX_NAVIGATION_SECONDS=90 \
	MAKOS_AARCH64_FIREFOX_INPUT_LIMIT_MS=500 \
	MAKOS_AARCH64_FIREFOX_SELECTION_LIMIT_MS=10000 \
	MAKOS_AARCH64_FIREFOX_CLIPBOARD=1 \
	MAKOS_AARCH64_FIREFOX_CLIPBOARD_SECONDS=120 \
	MAKOS_AARCH64_FIREFOX_LINK_CLICK=1 \
	MAKOS_AARCH64_FIREFOX_LINK_URI=https://www.iana.org/help/example-domains \
	MAKOS_AARCH64_FIREFOX_LINK_SECONDS=120 \
	MAKOS_AARCH64_FIREFOX_DOCUMENT_SELECTION=1 \
	MAKOS_AARCH64_FIREFOX_SELECTION_SECONDS=120 \
	MAKOS_AARCH64_FIREFOX_SUSTAINED_INTERACTION=1 \
	MAKOS_AARCH64_FIREFOX_SUSTAINED_CYCLES=2 \
	MAKOS_AARCH64_FIREFOX_SUSTAINED_NAVIGATION_SECONDS=120 \
	MAKOS_AARCH64_FIREFOX_SMP_REQUIRED=1 \
	MAKOS_AARCH64_FIREFOX_SERIAL_LOG="$(BUILD)/firefox-runtime-latest-serial.log" \
	MAKOS_QMP_SOCKETPAIR=1 \
	python3 scripts/boot_test_aarch64.py

test-aarch64-ipv6-runtime: image-aarch64
	MAKOS_AARCH64_SKIP_BROWSER_FETCH=1 \
	MAKOS_AARCH64_IPV6_PROBE=1 \
	python3 scripts/boot_test_aarch64.py

test-aarch64-package-runtime: image-aarch64
	python3 scripts/boot_test_aarch64_package.py

test-aarch64-gpt: image-aarch64-gpt
	MAKOS_AARCH64_GPT_IMAGE=$(AARCH64_GPT_IMAGE) python3 scripts/boot_test_aarch64_gpt.py

test-aarch64-install: image-aarch64
	MAKOS_AARCH64_IMAGE=$(AARCH64_IMAGE) python3 scripts/boot_test_aarch64_install.py

test-cpython-aarch64: image-aarch64 package-cpython-aarch64
	MAKOS_AARCH64_IMAGE=$(AARCH64_IMAGE) \
	MAKOS_AARCH64_PACKAGE_IMAGE=$(CPYTHON_AARCH64_DATA_IMAGE) \
	python3 scripts/boot_test_aarch64_cpython.py

unit:
	python3 scripts/test_uefi_kernel_handoff.py
	cargo test -p makos-boot-api
	cargo test -p makos-acpi
	cargo test -p makos-crypto
	cargo test -p makos-elf64
	cargo test -p makos-frame-allocator
	cargo test -p makos-gpt
	cargo test -p makos-installer
	cargo test -p makos-ipc
	python3 scripts/test_create_blank_install_target.py
	python3 scripts/test_aarch64_installer_resume.py
	cargo test -p makos-makfs4
	cargo test -p makos-makfs4-fsck
	cargo test -p makos-package-store
	python3 scripts/test_makfs4_block_io.py
	python3 scripts/test_makfs4_directory_scale.py
	cargo test -p makos-readiness
	cargo test -p makos-structured-log
	python3 scripts/test_aarch64_io_wake.py
	python3 scripts/test_aarch64_madvise.py
	python3 scripts/test_aarch64_scm_rights.py
	python3 scripts/test_aarch64_typed_ipc.py
	python3 scripts/test_cpu_affinity.py
	python3 scripts/test_aarch64_surface_priority.py
	python3 scripts/test_aarch64_firefox_interaction.py
	python3 scripts/test_aarch64_firefox_serial_log.py
	python3 scripts/test_firefox_provenance.py
	python3 scripts/test_firefox_errno.py
	python3 ports/firefox/test-print-settings.py
	python3 scripts/test_firefox_objdir.py
	python3 scripts/test_aarch64_firefox_trace_budget.py
	ports/firefox/test-toolchain.sh
	ports/firefox/test-build-mode.sh
	ports/firefox/test-host-tools.sh
	python3 scripts/test_aarch64_stack_protector.py
	python3 scripts/test_aarch64_package_probe.py
	python3 scripts/test_aarch64_symlink_timestamps.py
	python3 scripts/test_aarch64_robust_futex.py
	python3 scripts/test_aarch64_directed_signals.py
	python3 scripts/test_aarch64_timed_futex.py
	python3 scripts/test_aarch64_futex_requeue.py
	python3 scripts/test_aarch64_abi_discovery.py
	python3 scripts/test_structured_log_persistence.py
	python3 scripts/test_aarch64_cursor_plane.py
	python3 scripts/test_aarch64_smp.py
	python3 scripts/test_aarch64_smp_scheduler.py
	python3 scripts/test_aarch64_selfhost.py
	python3 scripts/test_aarch64_selfhost_parallel.py
	python3 scripts/test_aarch64_toolchain_freestanding.py
	cargo test -p makos-pe64
	python3 scripts/test_mkpackage.py
	python3 scripts/test_package_store_integration.py
	python3 scripts/test_integrated_data.py
	python3 scripts/test_mkgpt.py

check:
	python3 scripts/test_uefi_kernel_handoff.py
	cargo check -p makos-kernel --target x86_64-unknown-none
	cargo check -p makos-loader --target x86_64-unknown-uefi
	cargo check -p makos-kernel --target aarch64-unknown-none
	cargo check -p makos-loader --target aarch64-unknown-uefi
	cargo test -p makos-boot-api
	cargo test -p makos-acpi
	cargo test -p makos-crypto
	cargo test -p makos-elf64
	cargo test -p makos-frame-allocator
	cargo test -p makos-gpt
	cargo test -p makos-installer
	cargo test -p makos-ipc
	python3 scripts/test_create_blank_install_target.py
	python3 scripts/test_aarch64_installer_resume.py
	cargo test -p makos-makfs4
	cargo test -p makos-makfs4-fsck
	cargo test -p makos-package-store
	python3 scripts/test_makfs4_block_io.py
	python3 scripts/test_makfs4_directory_scale.py
	cargo test -p makos-readiness
	cargo test -p makos-structured-log
	python3 scripts/test_aarch64_io_wake.py
	python3 scripts/test_aarch64_madvise.py
	python3 scripts/test_aarch64_scm_rights.py
	python3 scripts/test_aarch64_typed_ipc.py
	python3 scripts/test_cpu_affinity.py
	python3 scripts/test_aarch64_surface_priority.py
	python3 scripts/test_aarch64_firefox_interaction.py
	python3 scripts/test_aarch64_firefox_serial_log.py
	python3 scripts/test_firefox_provenance.py
	python3 scripts/test_firefox_errno.py
	python3 ports/firefox/test-print-settings.py
	python3 scripts/test_firefox_objdir.py
	python3 scripts/test_aarch64_firefox_trace_budget.py
	ports/firefox/test-toolchain.sh
	ports/firefox/test-build-mode.sh
	ports/firefox/test-host-tools.sh
	python3 scripts/test_aarch64_stack_protector.py
	python3 scripts/test_aarch64_package_probe.py
	python3 scripts/test_aarch64_symlink_timestamps.py
	python3 scripts/test_aarch64_robust_futex.py
	python3 scripts/test_aarch64_directed_signals.py
	python3 scripts/test_aarch64_timed_futex.py
	python3 scripts/test_aarch64_futex_requeue.py
	python3 scripts/test_aarch64_abi_discovery.py
	python3 scripts/test_structured_log_persistence.py
	python3 scripts/test_aarch64_cursor_plane.py
	python3 scripts/test_aarch64_smp.py
	python3 scripts/test_aarch64_smp_scheduler.py
	python3 scripts/test_aarch64_selfhost.py
	python3 scripts/test_aarch64_selfhost_parallel.py
	python3 scripts/test_aarch64_toolchain_freestanding.py
	cargo test -p makos-pe64
	python3 scripts/test_mkpackage.py
	python3 scripts/test_package_store_integration.py
	python3 scripts/test_integrated_data.py
	python3 scripts/test_mkgpt.py

release: test test-x86_64-gpt test-x86_64-install test-makfs4-guest-fsck test-aarch64-gpt test-aarch64-install
	python3 scripts/mkdata.py $(DATA_IMAGE)
	python3 scripts/package_release.py

clean:
	cargo clean
	rm -rf $(BUILD)
