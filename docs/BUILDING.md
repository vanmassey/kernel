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

Obtain the official Limine binary release and place its BIOS/UEFI boot files and `limine` utility in a local directory. It should contain `limine-bios.sys`, `limine-bios-cd.bin`, `limine-uefi-cd.bin`, `BOOTX64.EFI`, and `limine`.

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
- The physical frame allocator is an early monotonic allocator; it does not free frames or establish virtual mappings.
- Do not add heap allocation until a physical/virtual memory strategy and allocator invariants are documented.
