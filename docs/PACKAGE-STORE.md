# Durable package store

`makos-package-store` is a `no_std`, sector-device transaction engine for
authenticated package payloads. Signature/repository authentication remains the
caller's responsibility. The engine owns payload persistence, catalog
validation, dependency/version validation, atomic replacement/removal, and
recovery.

## Region and format

Production layout version 1 reserves LBA 786,432–1,048,575 (384–512 MiB) on
the 1 GiB data volume. It contains two 131,072-sector (64 MiB) A/B slots.
Static package builder caps its payload below LBA 786,432; MakFS4 profile
allocation starts at LBA 1,048,576. Each slot contains:

- one CRC-32 header sector with format version, `PREPARING`/`COMMITTED` state,
  monotonic generation, entry count, and catalog CRC;
- four catalog sectors: up to eight fixed-size records;
- remaining sectors: sector-aligned package payloads, each with stored length
  and whole-payload CRC.

Names are 1–32 bytes (`[a-z][a-z0-9-]*`). Versions are numeric `major.minor` or
`major.minor.patch` values with 32-bit components and no leading zeroes. Each
package may require up to three packages using exact or minimum-version
constraints. Every install
or replacement validates the complete resulting dependency graph; missing,
incompatible, duplicate, self, and cyclic dependencies fail before disk writes.
Removal fails while any installed package depends on the target.

## Commit and recovery

Mutation builds a complete snapshot in the inactive slot:

1. Write `PREPARING` header; flush.
2. Copy unchanged payloads, write new payload, write catalog; flush.
3. Write CRC-protected `COMMITTED` header last; flush.

Opening validates both committed headers, catalogs, dependency graphs, payload
bounds, and every payload CRC, then selects the highest valid generation. A
partial inactive slot is ignored. A corrupt newest slot falls back to the older
complete generation. Existing active bytes are never overwritten by an
in-progress mutation.

Host tests use a volatile write cache plus crash injection at every mutation
boundary. They cover multi-sector persistence, replace/remove, dependency
constraints/cycles, interrupted first install/update, and corrupt-newest
fallback.

Rollback re-commits previous complete snapshot with newer generation; its own
interruption therefore retains current snapshot.

## Kernel integration and migration

Kernel `DataDisk` implements `SectorDevice`. After RSA-2048/SHA-256 manifest
verification and built-in `libc` resolution, package install/upgrade/query,
rollback, and removal use production store on disks large enough for complete
region. Existing small images retain legacy RAM behavior.

Migration is fail-closed and non-destructive:

- new image tools leave transaction region blank and reject static payload end
  beyond LBA 786,432;
- existing 1 GiB images initialize region only through first authenticated
  mutation when static header proves non-overlap;
- overlapping legacy header rejects mount; nonblank invalid transaction metadata
  rejects persistence; kernel never wipes/reformats either automatically;
- rebuilding static package through integrated-data tooling clears only
  package-owned range while retaining MakFS/profile hashes.

Signed syscall ABI retains legacy opaque `libc` compatibility, treated as an
implicit platform dependency, and now accepts this versioned dependency field:

`MAKDEP1\0 || count:u8 || (name_len:u8, kind:u8, version_len:u8, name, version)*`

Kind 0 means exact; kind 1 means at least. Up to three dynamic dependencies are
validated against complete resulting generation. Small-disk RAM fallback
rejects versioned graphs because it cannot represent multiple packages.

Active durable payloads mount read-only at `/packages/<name>/payload` on boot
and refresh after successful mutation. Existing immutable `/usr` package image
is never shadowed: static builder/kernel reject image paths under reserved
`/packages` prefix. Eight of 384 VFS package descriptors are reserved for this
namespace, so static image builder caps entries at 376. Open transaction file
descriptions capture compact size/LBA/sector backing. Package-store writes
refuse reuse of either A/B slot while any shared description still references
that slot, preserving old-FD bytes across replace/remove; callers retry after
closing the descriptor. Source compiles for both kernels and structural guards
pass; focused guest runtime proof remains pending. Settings SYSTEM card shows
disk/RAM/recovery backing, generation, and active count;
install/remove remain authenticated syscall operations, not unguarded GUI
buttons.

Still missing: guest boot fault injection, repository fetch, key rotation,
richer SAT/range solver, runtime open-FD pin qualification, recovery UI.
