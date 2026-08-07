# Pybor 🐍🦀💻

**Pybor** is an experimental, low-level programming language designed specifically for bare-metal operating system development. It combines the **clean and simple syntax of Python** with the future **borrowing/ownership model of Rust**.

Unlike other languages, Pybor does not simulate execution through a C compiler, nor does it require massive frameworks like LLVM. The compiler parses `.pyb` source code and directly generates **pure x86 Assembly**, ready to be linked into your own custom kernel.

## 🚀 Features
- **Python-like syntax:** Block indentation, clean definitions (e.g., `def kernel_main():`).
- **Zero dependencies (No standard library):** Designed for freestanding (bare-metal) environments. It does not require the C standard library (libc).
- **Direct Memory Access:** Direct reading and writing to memory using built-in functions (e.g., writing directly to the VGA text buffer).
- **Custom compiler:** Written entirely in Rust, with zero external dependencies. It directly generates a clean `.asm` file.

## 📂 Project Structure
- `src/` - Source code for the Pybor compiler itself (written in Rust).
- `test_os/boot/` - A Multiboot-compatible bootloader in Assembly (`boot.asm`).
- `test_os/kernel/` - A sample operating system kernel written in Pybor (`kernel.pyb`).
- `test_os/scripts/` - The linker script to build the final bootable `.bin` file.

## 🛠️ Prerequisites
To build the compiler and the OS itself, you will need:
- **Rust & Cargo** (to build the compiler)
- **NASM** (to assemble the bootloader and the generated assembly)
- **GNU Binutils / LD** (to link the object files)
- **QEMU** (optional, for testing/booting the OS)

## 🏁 Getting Started (Quick Start)

**1. Build the Pybor compiler**
```bash
cargo build --release
2. Compile the Pybor kernel to Assembly

./target/release/pybor test_os/kernel/kernel.pyb test_os/kernel/kernel.asm
3. Assemble the bootloader and kernel into object files

nasm -f elf32 test_os/boot/boot.asm -o test_os/boot/boot.o
nasm -f elf32 test_os/kernel/kernel.asm -o test_os/kernel/kernel.o
4. Link into a bootable kernel

ld -m elf_i386 -T test_os/scripts/linker.ld -o test_os/my_kernel.bin test_os/boot/boot.o test_os/kernel/kernel.o
5. Run the OS in QEMU

qemu-system-i386 -kernel test_os/my_kernel.bin
(If everything is correct, you will see the word "PYBOR" on the QEMU screen)

📜 Code Example (kernel.pyb)
This code writes colored text directly to the VGA video memory.

def kernel_main():
    # Print the text "PYBOR"
    poke16(0xb8000, 0x0f50) # 'P'
    poke16(0xb8002, 0x0f59) # 'Y'
    poke16(0xb8004, 0x0f42) # 'B'
    poke16(0xb8006, 0x0f4f) # 'O'
    poke16(0xb8008, 0x0f52) # 'R'
    
    # Halt the CPU (infinite loop)
    hang()
🗺️ Roadmap (explain: [D]=done, [N]=not done)
 - Direct translation to NASM Assembly[D]
 - Integration with a Multiboot bootloader[D]
 - Variables and basic operations (+, -, *, /)[N]
 - Loops (while, for) and conditionals (if)[N]
 - Rust-like Borrow/Ownership checker for pointer and memory management[N]
 - Drop the NASM intermediate step and generate direct ELF binary output[N]
Created as an experimental low-level language for OS development. 
