# Architecture notes

## Initial direction

The kernel is monolithic with modular internal subsystems. Rust is the primary language; assembly is reserved for architecture-specific entry, context switching, and operations that cannot be expressed safely or clearly in Rust.

## Bring-up sequence

1. Boot protocol and early diagnostics
2. Limine boot information and physical memory-map discovery
3. Usable physical-frame iteration and reservation
4. Initial monotonic physical frame allocator
5. CPU feature discovery and descriptor tables
6. Virtual memory and kernel address-space layout
7. Interrupt descriptor table and exception handlers
8. Local APIC and timer
9. Kernel heap
10. Scheduler and task model
11. System-call boundary and user mode
12. VFS and initial in-memory filesystem
13. Device-driver framework
14. Storage, networking, and userspace

## Safety rules

- Keep unsafe blocks as small as possible.
- Every unsafe block must state its safety rationale.
- Do not expose mutable global state without a documented synchronisation strategy.
- Keep architecture-specific code behind narrow interfaces.
- Add tests for pure logic and QEMU smoke tests for boot- and hardware-dependent behaviour.

## Current implementation

The kernel requests the Limine physical memory map, reports the number of entries and total usable memory over the legacy COM1 UART, and provides an iterator over usable 4 KiB physical frames. The initial physical-frame allocator consumes that iterator and returns each candidate frame at most once. It does not yet reclaim frames or create virtual mappings; those responsibilities belong to later memory-management stages.
