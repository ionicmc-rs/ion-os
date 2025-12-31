# Toolchain
CC      := x86_64-elf-gcc
LD      := x86_64-elf-ld
NASM    := nasm

# Source discovery
x86_64_asm_src_files    := $(shell find app/src/x86_64 -name '*.asm')
x86_64_c_src_files      := $(shell find app/src/kernel -name '*.c')
x86_64_rs_src_files     := $(shell find app/src/kernel -name '*.rs')

# Object names (mirror directory structure under build/x86_64)
x86_64_asm_obj_files    := $(patsubst app/src/x86_64/%.asm, build/x86_64/%.o, $(x86_64_asm_src_files))
x86_64_c_obj_files      := $(patsubst app/src/kernel/%.c, build/x86_64/kernel/%.o, $(x86_64_c_src_files))
x86_64_rs_obj_files := build/x86_64/kernel/ion_kernel.a

x86_64_obj_files        := $(x86_64_asm_obj_files) $(x86_64_c_obj_files) $(x86_64_rs_obj_files)

# Pattern rules

# ASM: build/x86_64/foo.o from app/src/x86_64/foo.asm
build/x86_64/%.o: app/src/x86_64/%.asm
	mkdir -p $(dir $@)
	$(NASM) -f elf64 $< -o $@

# C: build/x86_64/kernel/foo.o from app/src/kernel/foo.c
build/x86_64/kernel/%.o: app/src/kernel/%.c
	mkdir -p $(dir $@)
	$(CC) -c -I app/src/kernel/c_entry -ffreestanding -m64 -mno-red-zone -fno-omit-frame-pointer -fno-pic $< -o $@

build/x86_64/kernel/ion_kernel.a:
	mkdir -p $(dir $@) && \
	cd /root/env && \
	cargo build --no-default-features -p ion-kernel --target-dir build/x86_64/kernel/rust && \
	cp build/x86_64/kernel/rust/target/debug/libion_kernel.a $@

build/x86_64/kernel/ion_kernel_test.a:
	mkdir -p $(dir $@) && \
	cd /root/env && \
	cargo build --features test -p ion-kernel --target-dir build/x86_64/kernel/rust && \
	cp build/x86_64/kernel/rust/target/debug/libion_kernel.a $@

# Do nothing


# Final build target
.PHONY: build-x86_64 build-x86_64-test run-qemu run-qemu-tests clean clean-build clean-test
build-x86_64: $(x86_64_obj_files)
# clean

	mkdir -p dist/x86_64
	$(LD) -n -o dist/x86_64/kernel.bin -T app/targets/x86_64/linker.ld $(x86_64_obj_files)
	cp dist/x86_64/kernel.bin app/targets/x86_64/iso/boot/kernel.bin
	grub-mkrescue /usr/lib/grub/i386-pc -o dist/x86_64/kernel.iso app/targets/x86_64/iso
build-x86_64-test: $(x86_64_asm_obj_files) $(x86_64_c_obj_files) build/x86_64/kernel/ion_kernel_test.a
	mkdir -p dist/x86_64/test
	$(LD) -n -o dist/x86_64/test/kernel.bin -T app/targets/x86_64/linker.ld $(x86_64_asm_obj_files) $(x86_64_c_obj_files) build/x86_64/kernel/ion_kernel_test.a
	cp dist/x86_64/test/kernel.bin app/targets/x86_64/iso/boot/kernel.bin
	grub-mkrescue /usr/lib/grub/i386-pc -o dist/x86_64/test/kernel.iso app/targets/x86_64/iso

run-qemu:
	qemu-system-x86_64 dist/x86_64/kernel.iso -debugcon stdio -device isa-debug-exit,iobase=0xf4,iosize=0x04
run-qemu-tests:
	qemu-system-x86_64 dist/x86_64/test/kernel.iso -debugcon stdio -device isa-debug-exit,iobase=0xf4,iosize=0x04
clean:
	rm -f build/x86_64/kernel/ion_kernel.a build/x86_64/kernel/ion_kernel_test.a $(x86_64_asm_obj_files) $(x86_64_c_obj_files)
clean-build:
	make clean
	make build-x86_64
clean-test:
	make clean
	make build-x86_64-test