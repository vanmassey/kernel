# Architecture notes

## Initial direction

The kernel is monolithic with modular internal subsystems. Rust is the primary language; assembly is reserved for architecture-specific entry, context switching, and operations that cannot be expressed safely or clearly in Rust.

## Bring-up sequence

1. Boot protocol and early diagnostics
2. Limine boot information and physical memory-map discovery
3. Usable physical-frame iteration and reservation
4. Initial monotonic physical frame allocator
5. Emergency interrupt descriptor table
6. Per-exception diagnostics and CPU feature discovery
7. Limine framebuffer discovery and early display output
8. Active page-table inspection and virtual-address translation
9. Early kernel heap over the Limine HHDM
10. Managed page-table creation and kernel address-space layout
11. Rust `alloc` integration and allocation failure handling
12. Interrupt-controller setup and timer
14. Scheduler and task model
15. System-call boundary and user mode
16. VFS and initial in-memory filesystem
17. Device-driver framework
18. Storage, networking, and userspace

## Safety rules

- Keep unsafe blocks as small as possible.
- Every unsafe block must state its safety rationale.
- Do not expose mutable global state without a documented synchronisation strategy.
- Keep architecture-specific code behind narrow interfaces.
- Add tests for pure logic and QEMU smoke tests for boot- and hardware-dependent behaviour.

## Current implementation

The kernel requests the Limine physical memory map, reports the number of entries and total usable memory over the legacy COM1 UART, and provides an iterator over usable 4 KiB physical frames. The initial physical-frame allocator consumes that iterator and returns each candidate frame at most once. The x86_64 architecture layer also installs a 256-entry IDT; vectors 0–31 have dedicated assembly stubs that report the exception name, vector, error code, and instruction pointer over COM1 before halting. Other vectors still use the emergency halt stub; hardware interrupts are not yet handled. CPUID discovery reports the processor vendor, supported basic and extended leaves, and selected capabilities (SSE2, APIC, x2APIC, NX, 1 GiB pages, SYSCALL, and long mode). Physical frames are not yet reclaimed or mapped into virtual memory. The kernel now requests a Limine RGB framebuffer and draws a small Sestina startup screen directly into 32-bit framebuffer memory. A four-level page-table walker reads the active CR3 hierarchy and translates canonical virtual addresses through 4 KiB, 2 MiB, and 1 GiB mappings; it rejects five-level paging rather than misinterpreting it. The renderer validates dimensions, pitch, pixel format, and colour masks before writing pixels; serial output remains the diagnostic fallback. The kernel also reserves a contiguous 256 KiB region from usable physical frames and exposes it through an early aligned atomic bump allocator. Its startup self-test verifies alignment and read/write integrity. The heap does not yet reclaim freed allocations. Rust's `alloc` crate is now linked against this heap, with an allocation-error handler and a boot-time `Vec` self-test. The kernel can clone its active PML4, allocate a fresh hierarchy for a previously unused PML4 slot, map a reserved data frame at a new 4 KiB virtual address, switch CR3, and verify both the new mapping and its HHDM alias. The cloned root shares existing kernel mappings; it is not yet an isolated user address space.
