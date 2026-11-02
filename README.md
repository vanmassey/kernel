# Sestina

Sestina is a from-scratch operating-system kernel for x86_64, written primarily in Rust.

Sestina is built as a freestanding, `no_std` binary and uses Limine for boot. The project is organised around a small core with clearly separated architecture-specific code and kernel subsystems.

## Project direction

The goal is a capable, maintainable general-purpose operating system with its own kernel, memory manager, scheduler, device drivers, filesystem, and userspace.

| Area | Current direction |
|---|---|
| Architecture | x86_64 |
| Kernel design | Monolithic, internally modular |
| Main language | Rust (`no_std`) |
| Low-level code | x86_64 Assembly where required |
| Boot protocol | Limine |
| Development emulator | QEMU |
| Build system | Cargo, nightly Rust, GNU Make |
| Licence | GNU GPL v2 |

## Current development

**Current milestone — 0.10**

- Freestanding Rust target and nightly toolchain
- Limine request markers and kernel entry point
- Limine physical memory-map request and usable-memory summary
- Usable 4 KiB physical-frame iterator
- Initial monotonic physical-frame allocator
- 256-entry x86_64 interrupt descriptor table with dedicated diagnostic stubs for CPU exceptions 0–31 and an emergency handler for other vectors
- CPUID-based processor vendor and capability discovery (SSE2, APIC, x2APIC, NX, large pages, SYSCALL, long mode)
- Limine framebuffer discovery and an early RGB pixel/text renderer
- Four-level page-table walker for 4 KiB, 2 MiB, and 1 GiB mappings
- Initial 256 KiB kernel bump heap backed by contiguous usable physical frames and the Limine HHDM
- Rust `alloc` support with an allocation-error handler and a QEMU-tested `Vec` allocation path
- COM1 serial output for early diagnostics
- Kernel panic handler and CPU halt routine
- Linker script and boot configuration
- Automated formatting, compilation, and QEMU boot smoke tests with GitHub Actions

Milestone 0.8 added a direct RGB framebuffer renderer for the Sestina startup screen and inspection of the active four-level page tables. Milestone 0.9 added a 256 KiB early kernel heap backed by a contiguous run of usable physical frames. Milestone 0.10 connects Rust's `alloc` crate to that heap and verifies real `Vec` allocations during QEMU boot. The allocator is aligned and safe for concurrent reservations, but remains monotonic: freeing and page reclamation are not implemented. Next milestones cover managed page-table creation, interrupt-controller setup, and timers. The QEMU smoke test builds a bootable Limine ISO and checks that the kernel reaches its startup milestone.

## Build

Install nightly Rust and the required components:

```sh
rustup toolchain install nightly
rustup component add rust-src llvm-tools-preview --toolchain nightly
```

Build the kernel:

```sh
make build
```

For the complete ISO and QEMU workflow, see [docs/BUILDING.md](docs/BUILDING.md).

## Repository layout

```text
.cargo/                 Cargo target configuration
.github/workflows/      Continuous integration
docs/                   Build and architecture documentation
src/arch/x86_64/        x86_64-specific kernel code
src/main.rs             Kernel entry point and early console setup
src/display/             Framebuffer and early display code
Cargo.toml              Kernel package and build profiles
linker.ld               Kernel linker layout
limine.conf             Limine boot configuration
Makefile                Build, ISO, QEMU, and clean targets
```

## Engineering principles

1. Keep unsafe Rust small, explicit, and documented.
2. Make hardware and architecture assumptions visible.
3. Separate architecture-specific implementation from general kernel interfaces.
4. Test pure logic independently and verify hardware-dependent behaviour in QEMU.
5. Build stable foundations before adding higher-level functionality.

## Licence

This project is distributed under the GNU General Public License, version 2. See [LICENSE](LICENSE).
