use std::{env, fs, process};
use object::write::{Object, Symbol, SymbolSection};
use object::{Architecture, BinaryFormat, Endianness, SymbolFlags, SymbolKind, SymbolScope};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    X86_16, // Pro BIOS Bootloader (Raw Binary)
    X86_32, // Pro staré Kernely (ELF)
    X86_64, // Pro nové Kernely a UEFI (PE/COFF)
    Arm32,  // Budoucnost
    Arm64,  // Budoucnost
}

#[derive(Debug)]
enum Stmt { Poke16(u32, u16), Poke8(u32, u8), Hang }

fn fail(msg: &str) -> ! { eprintln!("{msg}"); process::exit(1); }

fn parse_int(s: &str) -> Result<u32, String> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).map_err(|_| format!("neplatné: {s}"))
    } else {
        s.parse::<u32>().map_err(|_| format!("neplatné: {s}"))
    }
}

fn parse_program(src: &str) -> Result<Vec<Stmt>, String> {
    let mut header_seen = false;
    let mut body = Vec::new();
    for raw_line in src.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        
        if trimmed == "def bootloader_main():" || trimmed == "def kernel_main():" {
            header_seen = true; continue;
        }
        if !header_seen { continue; }
        
        if trimmed == "hang()" { body.push(Stmt::Hang); continue; }
        
        if let Some(args) = trimmed.strip_prefix("poke16(").and_then(|s| s.strip_suffix(')')) {
            let p: Vec<&str> = args.split(',').map(|x| x.trim()).collect();
            body.push(Stmt::Poke16(parse_int(p[0])?, parse_int(p[1])? as u16)); continue;
        }
        
        if let Some(args) = trimmed.strip_prefix("poke8(").and_then(|s| s.strip_suffix(')')) {
            let p: Vec<&str> = args.split(',').map(|x| x.trim()).collect();
            body.push(Stmt::Poke8(parse_int(p[0])?, parse_int(p[1])? as u8)); continue;
        }
    }
    Ok(body)
}

// 1. BIOS Bootloader Backend (16-bit)
fn gen_x86_16(stmts: &[Stmt]) -> Vec<u8> {
    let mut code = Vec::new();
    for stmt in stmts {
        match *stmt {
            Stmt::Poke16(addr, value) => {
                // mov bx, addr (BB nn nn)
                code.push(0xBB); code.extend_from_slice(&(addr as u16).to_le_bytes());
                // mov ax, value (B8 nn nn)
                code.push(0xB8); code.extend_from_slice(&value.to_le_bytes());
                // mov [bx], ax (89 07)
                code.extend_from_slice(&[0x89, 0x07]);
            }
            Stmt::Poke8(addr, value) => {
                code.push(0xBB); code.extend_from_slice(&(addr as u16).to_le_bytes());
                code.push(0xB0); code.push(*value);
                code.extend_from_slice(&[0x88, 0x07]); // mov [bx], al
            }
            Stmt::Hang => code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]),
        }
    }
    code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]);
    
    // Mágie Bootloaderu: Doplníme nulama na 510 bytů a přidáme AA 55 boot signaturu
    let mut boot_sector = vec![0; 512];
    let copy_len = std::cmp::min(code.len(), 510);
    boot_sector[..copy_len].copy_from_slice(&code[..copy_len]);
    boot_sector[510] = 0x55;
    boot_sector[511] = 0xAA;
    
    boot_sector
}

// 2. x86_32 Backend
fn gen_x86_32(stmts: &[Stmt]) -> Vec<u8> {
    let mut code = Vec::new();
    for stmt in stmts {
        match *stmt {
            Stmt::Poke16(addr, value) => {
                code.push(0xBB); code.extend_from_slice(&addr.to_le_bytes());
                code.extend_from_slice(&[0x66, 0xB8]); code.extend_from_slice(&value.to_le_bytes());
                code.extend_from_slice(&[0x66, 0x89, 0x03]);
            }
            Stmt::Poke8(addr, value) => {
                code.push(0xBB); code.extend_from_slice(&addr.to_le_bytes());
                code.push(0xB0); code.push(*value);
                code.extend_from_slice(&[0x88, 0x03]);
            }
            Stmt::Hang => code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]),
        }
    }
    code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]);
    code
}

// 3. UEFI / x86_64 Backend
fn gen_x86_64(stmts: &[Stmt]) -> Vec<u8> {
    let mut code = Vec::new();
    for stmt in stmts {
        match *stmt {
            Stmt::Poke16(addr, value) => {
                code.extend_from_slice(&[0x48, 0xB8]);
                code.extend_from_slice(&(*addr as u64).to_le_bytes());
                code.extend_from_slice(&[0x66, 0xB9]); 
                code.extend_from_slice(&value.to_le_bytes());
                code.extend_from_slice(&[0x66, 0x89, 0x08]);
            }
            Stmt::Poke8(addr, value) => {
                code.extend_from_slice(&[0x48, 0xB8]);
                code.extend_from_slice(&(*addr as u64).to_le_bytes());
                code.push(0xB1); code.push(*value);
                code.extend_from_slice(&[0x88, 0x08]);
            }
            Stmt::Hang => code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]),
        }
    }
    code.extend_from_slice(&[0xB8, 0x00, 0x00, 0x00, 0x00, 0xC3]);
    code
}

// 4. Místa pro ARM (Zatím jen vrací Hang jako placeholder)
fn gen_arm(stmts: &[Stmt]) -> Vec<u8> {
    vec![0xFE, 0xE7] // b . (infinite loop na ARMu)
}

fn emit_file(machine_code: Vec<u8>, target: Target, output_file: &str) {
    if target == Target::X86_16 {
        // Bootloader musí být čistý `.bin` soubor (žádné hlavičky, jen opcody!)
        fs::write(output_file, machine_code).unwrap();
        return;
    }

    let (format, arch) = match target {
        Target::X86_32 => (BinaryFormat::Elf, Architecture::I386),
        Target::X86_64 => (BinaryFormat::Coff, Architecture::X86_64),
        Target::Arm32 => (BinaryFormat::Elf, Architecture::Arm),
        Target::Arm64 => (BinaryFormat::Elf, Architecture::Aarch64),
        _ => unreachable!(),
    };
    
    let mut obj = Object::new(format, arch, Endianness::Little);
    let text = obj.add_section(vec![], b".text".to_vec(), object::SectionKind::Text);
    let offset = obj.append_section_data(text, &machine_code, 16);

    let name: &[u8] = match target { 
        Target::X86_32 | Target::Arm32 | Target::Arm64 => b"kernel_main".as_slice(), 
        Target::X86_64 => b"efi_main".as_slice(),
        _ => b"main".as_slice(),
    };
    
    obj.add_symbol(Symbol {
        name: name.to_vec(), 
        value: offset, 
        size: machine_code.len() as u64,
        kind: SymbolKind::Text, 
        scope: SymbolScope::Dynamic,
        weak: false,
        section: SymbolSection::Section(text), 
        flags: SymbolFlags::None,
    });

    let bytes = obj.write().unwrap();
    fs::write(output_file, bytes).unwrap();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 { fail("Použití: pybor <architektura> <vstup.pyb> <výstup>\nCíle: x86_16, x86_32, x86_64, arm32, arm64"); }
    
    let target = match args[1].as_str() {
        "x86_16" => Target::X86_16,
        "x86_32" => Target::X86_32,
        "x86_64" => Target::X86_64,
        "arm32" => Target::Arm32,
        "arm64" => Target::Arm64,
        _ => fail("Neznámá architektura!"),
    };
    
    let src = fs::read_to_string(&args[2]).unwrap();
    let ast = parse_program(&src).unwrap();
    
    let mcode = match target {
        Target::X86_16 => gen_x86_16(&ast),
        Target::X86_32 => gen_x86_32(&ast),
        Target::X86_64 => gen_x86_64(&ast),
        Target::Arm32 | Target::Arm64 => gen_arm(&ast),
    };
    
    emit_file(mcode, target, &args[3]);
    println!("✅ Kompilace pro '{:?}' dokončena!", target);
}
