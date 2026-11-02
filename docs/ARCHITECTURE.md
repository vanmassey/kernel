# Architecture notes

## Initial direction

The kernel is monolithic with modular internal subsystems. Rust is the primary language; assembly is reserved for architecture-specific entry, context switching, and operations that cannot be expressed safely or clearly in Rust.

## Bring-up sequence

1. Boot protocol and early diagnostics
2. CPU feature discovery and descriptor tables
3. Physical memory map and frame allocator
4. Virtual memory and kernel address-space layout
5. Interrupt descriptor table and exception handlers
6. Local APIC and timer
7. Kernel heap
8. Scheduler and task model
9. System-call boundary and user mode
10. VFS and initial in-memory filesystem
11. Device-driver framework
12. Storage, networking, and userspace

## Safety rules

- Keep unsafe blocks as small as possible.
- Every unsafe block must state its safety rationale.
- Do not expose mutable global state without a documented synchronisation strategy.
- Keep architecture-specific code behind narrow interfaces.
- Add tests for pure logic and QEMU smoke tests for boot- and hardware-dependent behaviour.

## Current limitations

The foundation has no allocator, interrupt controller, scheduler, userspace, filesystem, or device model. The kernel currently emits early diagnostics over the legacy COM1 UART and halts. These are intentional boundaries, not implemented features.
