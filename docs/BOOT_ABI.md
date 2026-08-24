# Boot ABI v2

`crates/boot-api` is normative. All records use `repr(C)`, native little-endian
x86_64 fields, physical addresses, and fixed-width integers. Kernel first checks
`BOOT_INFO_MAGIC` and `BOOT_ABI_VERSION` before dereferencing nested pointers.

Memory-map pointer references final map returned while exiting UEFI boot
services. Entries have UEFI descriptor layout and stride `descriptor_size`;
stride must not be assumed equal to Rust struct size. Regions not marked UEFI
`CONVENTIONAL` stay reserved until subsystem-specific ownership transfer.

Framebuffer address is physical linear framebuffer. Pixel format values are
RGB, BGR, bitmask, or BLT-only. Current kernel renders only RGB/BGR.

ABI v2 appends a 192-byte inline boot-config buffer plus validated length.
Loader reads `/MAKOS.CFG` before exiting boot services, so no firmware pointer
survives handoff. Kernel currently requires `root=ata1`, `log=serial`, and
`makfs.recover=auto`; recovery option controls degraded MakFS repair policy.

Loader calls kernel entry as:

```text
extern "sysv64" fn(*const BootInfo) -> !
```

Entry executes with interrupts disabled by kernel immediately, UEFI x86_64
identity mappings active, no callable boot services, and loader stack active.
Milestone 2 replaces page tables and stack before reclaiming loader memory.
