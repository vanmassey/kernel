# Kernel

A from-scratch operating-system kernel for x86_64, written primarily in Rust.

Kernel is built as a freestanding, `no_std` binary and uses Limine for boot. The project is organised around a small core with clearly separated architecture-specific code and kernel subsystems.

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

**Foundation — 0.1**

- Freestanding Rust target and nightly toolchain
- Limine request markers and kernel entry point
- Limine physical memory-map request and usable-memory summary
- Usable 4 KiB physical-frame iterator
- Initial monotonic physical-frame allocator
- COM1 serial output for early diagnostics
- Kernel panic handler and CPU halt routine
- Linker script and boot configuration
- Automated formatting and compilation checks with GitHub Actions

The next development milestones cover CPU exceptions, interrupt handling, virtual memory, and the kernel heap.

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
src/main.rs             Kernel entry point and early serial console
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
