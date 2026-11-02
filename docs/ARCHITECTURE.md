# Architecture notes

## Initial direction

The kernel is monolithic with modular internal subsystems. Rust is the primary language; assembly is reserved for architecture-specific entry, context switching, and operations that cannot be expressed safely or clearly in Rust.

## Bring-up sequence

1. Boot protocol and early diagnostics
2. Limine boot information and physical memory-map discovery
3. Physical frame allocator
4. CPU feature discovery and descriptor tables
5. Virtual memory and kernel address-space layout
6. Interrupt descriptor table and exception handlers
7. Local APIC and timer
8. Kernel heap
9. Scheduler and task model
10. System-call boundary and user mode
11. VFS and initial in-memory filesystem
12. Device-driver framework
13. Storage, networking, and userspace

## Safety rules

- Keep unsafe blocks as small as possible.
- Every unsafe block must state its safety rationale.
- Do not expose mutable global state without a documented synchronisation strategy.
- Keep architecture-specific code behind narrow interfaces.
- Add tests for pure logic and QEMU smoke tests for boot- and hardware-dependent behaviour.

## Current limitations

The kernel currently requests the Limine physical memory map and reports the number of entries and total usable memory over the legacy COM1 UART. Physical frame allocation, interrupt handling, scheduling, userspace, filesystems, and the device model are the next major subsystems.
