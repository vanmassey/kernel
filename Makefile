TARGET := x86_64-unknown-none
KERNEL := target/$(TARGET)/debug/kernel
ISO_ROOT := build/iso_root
LIMINE_VERSION := v12.9.6
LIMINE_ARCHIVE := build/limine-binary.tar.gz
LIMINE_DIR ?=
# Recursive expansion is intentional: after limine-setup extracts the archive,
# this finds the actual directory even when the archive contains a versioned folder.
LIMINE_PATH = $(if $(LIMINE_DIR),$(LIMINE_DIR),$(shell find limine build/limine -type f -name limine-bios.sys -print -quit 2>/dev/null | sed 's|/limine-bios.sys$$||'))

.PHONY: all build check fmt verify clean limine-setup iso run demo boot-test exception-test

all: build

build:
	cargo +nightly build -Z build-std=core,alloc,compiler_builtins $(if $(FEATURES),--features $(FEATURES),)

check:
	cargo +nightly check -Z build-std=core,alloc,compiler_builtins

fmt:
	cargo +nightly fmt --all -- --check

verify: fmt build

# Prepare Limine automatically so "make run" works after the host tools and
# Rust toolchain are installed. An explicit LIMINE_DIR still takes precedence.
limine-setup:
	@if [ -n "$(LIMINE_DIR)" ]; then \
		test -f "$(LIMINE_DIR)/limine-bios.sys" || (echo "LIMINE_DIR does not contain limine-bios.sys: $(LIMINE_DIR)" && exit 1); \
		make -C "$(LIMINE_DIR)"; \
	else \
		mkdir -p build/limine; \
		curl --fail --location --silent --show-error \
			https://github.com/Limine-Bootloader/Limine/releases/download/$(LIMINE_VERSION)/limine-binary.tar.gz \
			--output "$(LIMINE_ARCHIVE)"; \
		tar -xzf "$(LIMINE_ARCHIVE)" -C build/limine; \
		limine_dir="$$(dirname "$$(find build/limine -type f -name limine-bios.sys -print -quit)")"; \
		test -n "$$limine_dir" || (echo "Could not find limine-bios.sys in the downloaded Limine release." && exit 1); \
		make -C "$$limine_dir"; \
	fi

iso: build limine-setup
	@test -f "$(KERNEL)" || (echo "Kernel ELF not found: $(KERNEL)" && exit 1)
	@test -f "$(LIMINE_PATH)/limine-bios.sys" || (echo "Limine BIOS files not found; check LIMINE_DIR." && exit 1)
	rm -rf "$(ISO_ROOT)"
	mkdir -p "$(ISO_ROOT)/boot/limine" "$(ISO_ROOT)/EFI/BOOT"
	cp "$(KERNEL)" "$(ISO_ROOT)/boot/kernel.elf"
	cp limine.conf "$(ISO_ROOT)/boot/limine/limine.conf"
	cp "$(LIMINE_PATH)/limine-bios.sys" "$(ISO_ROOT)/boot/limine/"
	cp "$(LIMINE_PATH)/limine-bios-cd.bin" "$(ISO_ROOT)/boot/limine/"
	cp "$(LIMINE_PATH)/limine-uefi-cd.bin" "$(ISO_ROOT)/boot/limine/"
	cp "$(LIMINE_PATH)/BOOTX64.EFI" "$(ISO_ROOT)/EFI/BOOT/"
	xorriso -as mkisofs -R -r -J -b boot/limine/limine-bios-cd.bin -no-emul-boot -boot-load-size 4 -boot-info-table --efi-boot boot/limine/limine-uefi-cd.bin -efi-boot-part --efi-boot-image --protective-msdos-label "$(ISO_ROOT)" -o build/kernel.iso
	"$(LIMINE_PATH)/limine" bios-install build/kernel.iso

run: iso
	qemu-system-x86_64 -M q35 -m 256M -cdrom build/kernel.iso -boot order=d -serial stdio -no-reboot -no-shutdown

# Full first-demonstration path: verify, boot-test, exception-test, then launch QEMU.
demo:
	$(MAKE) verify
	$(MAKE) boot-test
	$(MAKE) exception-test FEATURES=exception-test
	$(MAKE) run

boot-test: iso
	@set +e; timeout 15s qemu-system-x86_64 -M q35 -m 256M -cdrom build/kernel.iso -boot order=d -serial stdio -display none -no-reboot -no-shutdown > build/qemu-serial.log 2>&1; status=$$?; set -e; if [ $$status -ne 124 ] && [ $$status -ne 0 ]; then cat build/qemu-serial.log; exit $$status; fi
	@echo "--- QEMU serial output ---"; cat build/qemu-serial.log
	@grep -F "SESTINA :: x86_64 kernel" build/qemu-serial.log
	@grep -F "[cpu] Exception diagnostics enabled (vectors 0-31)" build/qemu-serial.log
	@grep -F "[cpu] Vendor:" build/qemu-serial.log
	@grep -F "[cpu] NX:" build/qemu-serial.log
	@grep -F "[pci] Enumeration complete:" build/qemu-serial.log
	@grep -F "[memory] HHDM offset:" build/qemu-serial.log
	@grep -F "[memory] Direct-map virtual address:" build/qemu-serial.log
	@grep -F "[memory] Active page-table translation:" build/qemu-serial.log
	@grep -F "[memory] Kernel heap ready:" build/qemu-serial.log
	@grep -F "[memory] Heap allocator self-test: passed" build/qemu-serial.log
	@grep -F "[memory] Rust Vec self-test: passed" build/qemu-serial.log
	@grep -F "[filesystem] RAMFS self-test: passed" build/qemu-serial.log
	@grep -F "[memory] New PML4 active:" build/qemu-serial.log
	@grep -F "[memory] 4 KiB page mapping self-test: passed" build/qemu-serial.log
	@grep -F "[memory] General 4 KiB mapper self-test: passed" build/qemu-serial.log
	@grep -F "[boot] Milestone 0.14: timer and PS/2 keyboard polling" build/qemu-serial.log
	@grep -F "[interrupt] PIC remapped; only IRQ0 unmasked" build/qemu-serial.log
	@grep -F "[timer] PIT configured at 100 Hz" build/qemu-serial.log
	@grep -F "[timer] Interrupt self-test passed:" build/qemu-serial.log
	@grep -F "[keyboard] PS/2 polling interface ready; IRQ1 remains masked" build/qemu-serial.log
	@grep -F "[keyboard] Scancode decoder self-test: passed" build/qemu-serial.log
	@grep -F "[shell] Serial command shell ready" build/qemu-serial.log
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
