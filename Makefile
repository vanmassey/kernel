TARGET := x86_64-unknown-none
KERNEL := target/$(TARGET)/debug/kernel
ISO_ROOT := build/iso_root
LIMINE_DIR ?= limine

.PHONY: all build check clean iso run boot-test exception-test

all: build

build:
	cargo +nightly build -Z build-std=core,compiler_builtins $(if $(FEATURES),--features $(FEATURES),)

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
	qemu-system-x86_64 -M q35 -m 256M -cdrom build/kernel.iso -boot order=d -serial stdio -no-reboot -no-shutdown

boot-test: iso
	@set +e; timeout 15s qemu-system-x86_64 -M q35 -m 256M -cdrom build/kernel.iso -boot order=d -serial stdio -display none -no-reboot -no-shutdown > build/qemu-serial.log 2>&1; status=$$?; set -e; if [ $$status -ne 124 ] && [ $$status -ne 0 ]; then cat build/qemu-serial.log; exit $$status; fi
	@echo "--- QEMU serial output ---"; cat build/qemu-serial.log
	@grep -F "SESTINA :: x86_64 kernel" build/qemu-serial.log
	@grep -F "[cpu] Exception diagnostics enabled (vectors 0-31)" build/qemu-serial.log
	@grep -F "[cpu] Vendor:" build/qemu-serial.log
	@grep -F "[cpu] NX:" build/qemu-serial.log
	@grep -F "[memory] HHDM offset:" build/qemu-serial.log
	@grep -F "[memory] Direct-map virtual address:" build/qemu-serial.log
	@grep -F "[memory] Active page-table translation:" build/qemu-serial.log
	@grep -F "[memory] Active page-table translation:" build/qemu-serial.log
	@grep -F "[memory] Active page-table translation:" build/qemu-serial.log
	@grep -F "[boot] Milestone 0.8: framebuffer and paging inspection" build/qemu-serial.log
	@grep -F "[display] Framebuffer online:" build/qemu-serial.log
	@echo "QEMU boot smoke test passed."

exception-test: iso
	@set +e; timeout 5s qemu-system-x86_64 -M q35 -m 256M -cdrom build/kernel.iso -boot order=d -serial stdio -display none -no-reboot -no-shutdown > build/qemu-exception.log 2>&1; status=$$?; set -e; if [ $$status -ne 124 ] && [ $$status -ne 0 ]; then cat build/qemu-exception.log; exit $$status; fi
	@echo "--- QEMU exception-test serial output ---"; cat build/qemu-exception.log
	@grep -F "[CPU EXCEPTION] Breakpoint (vector 3)" build/qemu-exception.log
	@grep -F "[exception] Error code:" build/qemu-exception.log
	@grep -F "[exception] Instruction pointer:" build/qemu-exception.log
	@echo "QEMU exception self-test passed."

clean:
	cargo clean
	rm -rf build
