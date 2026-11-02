TARGET := x86_64-unknown-none
KERNEL := target/$(TARGET)/debug/kernel
ISO_ROOT := build/iso_root
LIMINE_DIR ?= limine

.PHONY: all build check clean iso run

all: build

build:
	cargo +nightly build -Z build-std=core,compiler_builtins

check:
	cargo +nightly check -Z build-std=core,compiler_builtins

iso: build
	@test -f "$(KERNEL)" || (echo "Kernel ELF not found: $(KERNEL)" && exit 1)
	@test -d "$(LIMINE_DIR)" || (echo "Set LIMINE_DIR to a directory containing Limine boot files and utility." && exit 1)
	rm -rf "$(ISO_ROOT)"
	mkdir -p "$(ISO_ROOT)/boot/limine" "$(ISO_ROOT)/EFI/BOOT"
	cp "$(KERNEL)" "$(ISO_ROOT)/boot/kernel.elf"
	cp limine.conf "$(ISO_ROOT)/boot/limine/limine.conf"
	cp "$(LIMINE_DIR)/limine-bios.sys" "$(ISO_ROOT)/boot/limine/"
	cp "$(LIMINE_DIR)/limine-bios-cd.bin" "$(ISO_ROOT)/boot/limine/"
	cp "$(LIMINE_DIR)/limine-uefi-cd.bin" "$(ISO_ROOT)/boot/limine/"
	cp "$(LIMINE_DIR)/BOOTX64.EFI" "$(ISO_ROOT)/EFI/BOOT/"
	xorriso -as mkisofs -R -r -J -b boot/limine/limine-bios-cd.bin -no-emul-boot -boot-load-size 4 -boot-info-table --efi-boot boot/limine/limine-uefi-cd.bin -efi-boot-part --efi-boot-image --protective-msdos-label "$(ISO_ROOT)" -o build/kernel.iso
	"$(LIMINE_DIR)/limine" bios-install build/kernel.iso

run: iso
	qemu-system-x86_64 -M q35 -m 256M -cdrom build/kernel.iso -serial stdio -no-reboot -no-shutdown

clean:
	cargo clean
	rm -rf build
