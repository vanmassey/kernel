# Building, testing, and booting

This guide describes the commands currently provided by the repository's
Makefile and the checks configured in GitHub Actions. Run commands from the
repository root unless a section says otherwise.

## 1. Host requirements

Sestina is a freestanding Rust kernel for 64-bit x86 PCs. You need a Rust
toolchain and several host tools to compile the kernel, create a bootable ISO,
and run it in an emulator.

On Linux Mint or Ubuntu, install the system packages with:

```sh
sudo apt update
sudo apt install --yes build-essential curl git make xorriso qemu-system-x86 coreutils
```

These packages provide the compiler and build utilities, Git, ISO creation
tools, QEMU, and the `timeout` command used by the automated tests.

Check that the main tools are available:

```sh
git --version
make --version
qemu-system-x86_64 --version
xorriso -version
timeout --version
```

If a command is reported as not found, install the package that provides it
before continuing.

## 2. Rust toolchain

The kernel targets `x86_64-unknown-none`. This is a bare-metal target: it
does not assume that Linux, Windows, or another host operating system is
present underneath the kernel.

The repository contains a `rust-toolchain.toml` file selecting nightly Rust
and the `x86_64-unknown-none` target. Install the toolchain and the components
used by this project:

```sh
rustup toolchain install nightly
rustup target add x86_64-unknown-none --toolchain nightly
rustup component add rust-src llvm-tools-preview rustfmt --toolchain nightly
```

Check the installed versions and target:

```sh
rustup show
rustc +nightly --version
cargo +nightly --version
rustup target list --installed --toolchain nightly
rustup component list --installed --toolchain nightly
```

The build uses nightly Rust because Cargo is asked to build parts of the Rust
core libraries for the bare-metal target. The required build-std settings are
stored in `.cargo/config.toml`.

## 3. Get the source code

If you have not cloned the repository yet:

```sh
git clone https://github.com/vanmassey/kernel.git
cd kernel
```

If you already have a checkout, enter its directory and update your view of
the remote branches:

```sh
cd /path/to/kernel
git status
git fetch origin
git branch --show-current
```

To switch to the main branch and update it:

```sh
git switch main
git pull --ff-only origin main
```

Do not run the update command if you have uncommitted changes you need to keep.
Check `git status` first.

## 4. Compile and check the kernel

### Build

Compile the kernel:

```sh
make build
```

The resulting ELF executable is:

```text
target/x86_64-unknown-none/debug/kernel
```

### Check without producing the final executable

Run Cargo's check pass:

```sh
make check
```

This catches many Rust errors more quickly than a full link, but it does not
replace a successful build.

### Check formatting

Check Rust formatting without changing files:

```sh
make fmt
```

If formatting needs to be corrected, apply rustfmt with:

```sh
cargo +nightly fmt --all
```

Then check it again:

```sh
make fmt
```

### Run the repository's standard verification

```sh
make verify
```

This runs the formatting check followed by the kernel build. It is the
standard local check used by the build job in GitHub Actions.

### Inspect the build output

On systems with GNU binutils installed, these commands can help inspect the
kernel executable:

```sh
file target/x86_64-unknown-none/debug/kernel
readelf -h target/x86_64-unknown-none/debug/kernel
```

The ELF header should identify a 64-bit executable for the x86-64 machine
architecture.

## 5. Prepare Limine

Sestina currently boots through the Limine x86_64 boot protocol. The ISO
creation target expects a Limine binary-release directory containing the
boot files and the Limine host utility.

Download the official Limine binary release from:

https://github.com/Limine-Bootloader/Limine/releases

Extract the archive. For example, if it is in your Downloads directory:

```sh
mkdir -p "$HOME/limine"
tar -xf "$HOME/Downloads/limine-binary.tar.gz" -C "$HOME/limine"
```

The extracted archive may contain a versioned subdirectory. Locate the
directory containing `limine-bios.sys`:

```sh
find "$HOME/limine" -type f -name limine-bios.sys -print
```

Set `LIMINE_DIR` to the directory containing that file. For example, if the
command prints `/home/user/limine/limine-binary`, use:

```sh
export LIMINE_DIR="$HOME/limine/limine-binary"
```

Use the actual directory printed on your machine. Check that the required
files are present:

```sh
ls "$LIMINE_DIR"
```

The directory must contain these files:

- `limine-bios.sys`
- `limine-bios-cd.bin`
- `limine-uefi-cd.bin`
- `BOOTX64.EFI`

The release includes the Limine host utility source. Build that utility:

```sh
make -C "$LIMINE_DIR"
```

Check that the executable was produced:

```sh
test -x "$LIMINE_DIR/limine" && echo "Limine utility is ready"
```

Keep the `LIMINE_DIR` variable set in any terminal where you build the ISO.
You can also provide it directly to each Make command.

## 6. Create a bootable ISO

Build the kernel and create the hybrid BIOS/UEFI ISO:

```sh
make iso LIMINE_DIR="$LIMINE_DIR"
```

The ISO is written to:

```text
build/kernel.iso
```

Check that it exists:

```sh
ls -lh build/kernel.iso
file build/kernel.iso
```

The ISO target copies the kernel ELF, `limine.conf`, and the required Limine
boot files into a staging directory before creating the image with xorriso.
It then runs Limine's BIOS installation command on the ISO.

## 7. Run Sestina in QEMU

Start the kernel in an interactive QEMU window:

```sh
make run LIMINE_DIR="$LIMINE_DIR"
```

The Makefile currently configures QEMU with:

- A q35 machine model.
- 256 MiB of emulated RAM.
- The generated ISO as a CD-ROM.
- Boot from the CD-ROM.
- COM1 serial output connected to the terminal.
- Automatic reboot and shutdown disabled.

To leave QEMU, close its window or use the QEMU monitor if you have enabled
one. The current `make run` command does not configure a QEMU monitor on a
separate terminal.

The kernel's serial console uses COM1 at 38400 baud, 8 data bits, no parity,
and one stop bit.

## 8. Automated QEMU boot test

Run the boot smoke test:

```sh
make boot-test LIMINE_DIR="$LIMINE_DIR"
```

This target:

1. Builds the kernel and ISO.
2. Starts QEMU without a graphical display.
3. Allows it to run for up to 15 seconds.
4. Saves serial output to `build/qemu-serial.log`.
5. Checks for expected boot, CPU, memory, timer, keyboard, and shell messages.

Read the captured log:

```sh
cat build/qemu-serial.log
```

Search for errors or important milestones:

```sh
grep -niE 'error|failed|panic|exception|passed|ready|milestone' build/qemu-serial.log
```

The test is a boot smoke test. It verifies that expected startup messages
appear; it does not prove that every device or interactive shell feature
works correctly.

## 9. CPU exception test

The kernel has an optional `exception-test` Cargo feature used to trigger a
controlled CPU breakpoint exception during testing.

Run the test with:

```sh
make exception-test FEATURES=exception-test LIMINE_DIR="$LIMINE_DIR"
```

The test creates an ISO with that feature enabled, boots it in QEMU, captures
serial output in `build/qemu-exception.log`, and checks for the expected
breakpoint diagnostic messages.

Inspect the log if the test fails:

```sh
cat build/qemu-exception.log
```

This is an intentional exception test. Do not enable the feature for an
ordinary kernel boot unless you specifically want to run that test.

## 10. Useful development commands

| Command | Purpose |
|---|---|
| `make` | Default target; builds the kernel |
| `make build` | Compile the kernel |
| `make check` | Run Cargo's check pass |
| `make fmt` | Check Rust formatting |
| `cargo +nightly fmt --all` | Apply Rust formatting |
| `make verify` | Check formatting and build |
| `make iso LIMINE_DIR="$LIMINE_DIR"` | Create a bootable ISO |
| `make run LIMINE_DIR="$LIMINE_DIR"` | Run interactively in QEMU |
| `make demo LIMINE_DIR="$LIMINE_DIR"` | Verify, run both QEMU tests, then launch the interactive demonstration |
| `make boot-test LIMINE_DIR="$LIMINE_DIR"` | Run the boot smoke test |
| `make exception-test FEATURES=exception-test LIMINE_DIR="$LIMINE_DIR"` | Run the exception test |
| `make clean` | Remove Cargo and generated build output |
| `make -n build` | Print the build commands without running them |
| `git status` | Show changed and untracked files |
| `git diff` | Show unstaged changes |
| `git diff --check` | Check the diff for whitespace errors |
| `git log --oneline -5` | Show the five latest commits |

After changing source code, a sensible quick check is:

```sh
make fmt
make check
make build
```

Before opening a pull request, run:

```sh
make verify
git diff --check
git status
```

## 11. GitHub Actions

The workflow is defined in `.github/workflows/kernel.yml`.

On pushes to `main` and branches matching `foundation/**`, and on pull
requests, the workflow runs the build job. That job installs nightly Rust
with the required components and runs:

```sh
make verify
```

The `boot-smoke` job runs for pull requests, pushes to `main`, and when the
workflow is started manually with the `workflow_dispatch` event. It installs QEMU and xorriso,
downloads the pinned Limine v12.9.6 binary release, builds the ISO, runs
`make boot-test`, and then runs `make exception-test`.

A green build job confirms formatting and compilation. The boot-smoke job
additionally checks that the ISO reaches the expected startup milestones in
QEMU, including the RAMFS self-test. The exception test checks the controlled
breakpoint diagnostic path.

To run the workflow manually from GitHub:

1. Open the repository's **Actions** tab.
2. Select **Kernel CI**.
3. Choose **Run workflow**.
4. Select the branch you want to test.
5. Start the workflow and inspect both jobs.

## 12. Troubleshooting

### `rustup: command not found`

Rust and rustup are not installed or are not on your shell's PATH. Install
rustup using the official instructions at:

https://rustup.rs/

Open a new terminal after installation if the command is still unavailable.

### Missing Rust target or component

Run:

```sh
rustup target add x86_64-unknown-none --toolchain nightly
rustup component add rust-src llvm-tools-preview rustfmt --toolchain nightly
```

### `make: command not found`

On Linux Mint or Ubuntu:

```sh
sudo apt install --yes make build-essential
```

### `qemu-system-x86_64: command not found`

On Linux Mint or Ubuntu:

```sh
sudo apt install --yes qemu-system-x86
```

### `xorriso: command not found`

On Linux Mint or Ubuntu:

```sh
sudo apt install --yes xorriso
```

### Limine files cannot be found

Check the value of `LIMINE_DIR` and locate the BIOS file:

```sh
printf 'LIMINE_DIR=%s\n' "$LIMINE_DIR"
find "$HOME/limine" -type f -name limine-bios.sys -print
```

Set `LIMINE_DIR` to the directory containing the Limine boot files and
the built `limine` executable.

### The ISO target says the kernel ELF is missing

Build the kernel first:

```sh
make build
```

Then retry the ISO command.

### The boot test fails

Read the complete serial log:

```sh
cat build/qemu-serial.log
```

Check whether QEMU itself is installed and whether the Limine directory is
correct. A missing expected log line may also mean the kernel stopped during
an earlier initialisation stage.

### Start from a clean build

If generated files are stale or a previous build has left confusing output:

```sh
make clean
make verify
```

Then rebuild the ISO:

```sh
make iso LIMINE_DIR="$LIMINE_DIR"
```

## 13. Current kernel limitations

The following are implementation limitations, not build errors:

- The kernel uses the x86_64 Limine boot protocol.
- COM1 is configured for 38400 baud, 8 data bits, no parity, and one stop bit.
- The physical-frame allocator is monotonic and does not free frames.
- The initial 256 KiB heap is backed by contiguous usable frames accessed
  through the Limine HHDM. It supports aligned bump allocation and Rust
  `alloc` collections, but does not reclaim deallocated memory.
- The heap is an early kernel facility, not a general-purpose allocator for
  long-running workloads.
- The current address-space test clones the active kernel PML4 and adds one
  private 4 KiB mapping. Existing kernel page tables are shared; this is not
  yet process isolation.
- The PS/2 keyboard is currently polled; IRQ1 remains masked.
- Timer interrupts are configured through the PIT and PIC. The shell enables
  maskable interrupts so the timer continues ticking while it waits for input.
- The RAMFS is volatile, has no disk-backed persistence, and currently has
  no shell commands or deletion/rename operations.
- The current shell and device support are early kernel facilities, not a
  complete user-space operating environment.

These limitations should be updated as the implementation changes.
