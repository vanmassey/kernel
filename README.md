# Kernel

A from-scratch x86_64 operating-system kernel written primarily in Rust.

The project is in early bring-up. The first objective is a small, auditable kernel that boots under Limine, writes diagnostics to a serial console, and provides a stable foundation for memory management, interrupts, scheduling, drivers, filesystems, and networking.

## Design

- Architecture: x86_64 (initial target)
- Kernel design: monolithic, internally modular
- Primary language: Rust (no_std)
- Low-level language: x86_64 Assembly where required
- Boot protocol: Limine
- Emulator: QEMU
- Build: Cargo, nightly Rust, GNU Make
- Licence: GNU GPL v2 (see LICENSE)

## Status

Milestone 0.1 — Foundation (in progress). This milestone establishes the freestanding build, Limine entry point, COM1 serial diagnostics, panic handling, and developer documentation. A successful compile is not a successful boot: QEMU testing is required before declaring the milestone complete.

## Prerequisites

Install nightly Rust with rust-src and llvm-tools-preview, QEMU, GNU Make, xorriso, and the Limine bootloader binaries/tooling.

    rustup toolchain install nightly
    rustup component add rust-src llvm-tools-preview --toolchain nightly
    make build

See docs/BUILDING.md for the complete workflow.

## Project layout

- src/ — kernel entry point and early boot code
- src/arch/x86_64/ — architecture-specific code
- docs/ — architecture and development notes

## Engineering principles

1. Keep unsafe Rust small, explicit, and documented.
2. Make hardware and architecture assumptions visible.
3. Prefer tested interfaces over premature feature breadth.
4. Do not claim a subsystem works until it has a test or reproducible manual verification.
5. Keep boot, architecture, memory, scheduling, drivers, and filesystems separated by clear interfaces.

This is an independent project. It is not Linux and does not aim to be Linux-compatible at this stage.
