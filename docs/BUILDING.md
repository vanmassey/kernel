# Building and booting

## Toolchain

The kernel is a freestanding Rust binary and builds core for x86_64-unknown-none using nightly Rust.

    rustup toolchain install nightly
    rustup component add rust-src llvm-tools-preview --toolchain nightly

Install QEMU, GNU Make, xorriso, and the `timeout` utility using your distribution's package manager.

## Compile

    make build

The ELF image is written to target/x86_64-unknown-none/debug/kernel.

## Boot with Limine

Obtain the official Limine binary release and extract it. The release includes the Limine host utility as source, so build that utility once with `make -C /path/to/limine`. The directory should then contain `limine-bios.sys`, `limine-bios-cd.bin`, `limine-uefi-cd.bin`, `BOOTX64.EFI`, and the built `limine` executable.

Build the hybrid BIOS/UEFI ISO:

    make iso LIMINE_DIR=/path/to/limine

Run it interactively in QEMU:

    make run LIMINE_DIR=/path/to/limine

## Automated boot smoke test

Run the kernel in QEMU for up to 15 seconds and check that its serial console reaches the startup milestone:

    make boot-test LIMINE_DIR=/path/to/limine

The test captures serial output in `build/qemu-serial.log`. It succeeds only if the kernel banner, emergency IDT message, and milestone message are present.

GitHub Actions runs this test on pushes and pull requests. The workflow downloads the pinned Limine v12.9.6 binary release, builds the ISO, and boots it under QEMU.

## Notes

- The entry point assumes the x86_64 Limine protocol.
- COM1 is configured for 38400 baud, 8 data bits, no parity, one stop bit.
- Interrupts are not enabled yet. The kernel deliberately halts after startup messages.
- The physical-frame allocator is monotonic and does not free frames.
- The initial 256 KiB heap is backed by contiguous usable frames accessed through the Limine HHDM. It supports aligned bump allocation but does not reclaim deallocated memory.
- The heap is an early kernel facility, not yet a general-purpose allocator for long-running workloads.
