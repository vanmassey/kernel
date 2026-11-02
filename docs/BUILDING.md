# Building and booting

## Toolchain

The kernel is a freestanding Rust binary and builds core for x86_64-unknown-none using nightly Rust.

    rustup toolchain install nightly
    rustup component add rust-src llvm-tools-preview --toolchain nightly

Install QEMU, GNU Make, and xorriso using your distribution's package manager.

## Compile

    make build

The ELF image is written to target/x86_64-unknown-none/debug/kernel.

## Boot with Limine

Obtain the official Limine binary release and place its BIOS/UEFI boot files and limine utility in a local directory. It should contain limine-bios.sys, limine-bios-cd.bin, limine-uefi-cd.bin, BOOTX64.EFI, and limine.

    make iso LIMINE_DIR=/path/to/limine
    make run LIMINE_DIR=/path/to/limine

QEMU's serial output should show the kernel banner and boot messages. This is the first manual boot acceptance test. A successful build alone does not prove the image boots.

## Notes

- The entry point assumes the x86_64 Limine protocol.
- COM1 is configured for 38400 baud, 8 data bits, no parity, one stop bit.
- Interrupts are not enabled yet. The kernel deliberately halts after startup messages.
- Do not add heap allocation until a physical/virtual memory strategy and allocator invariants are documented.
