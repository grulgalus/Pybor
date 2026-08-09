use std::{env, fs, process};
use object::write::{Object, Symbol, SymbolSection};
use object::{Architecture, BinaryFormat, Endianness, SymbolFlags, SymbolKind, SymbolScope};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Target { Bios32, Uefi64 }

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
        
        if !header_seen {
            if trimmed != "def kernel_main():" { return Err("očekávám `def kernel_main():`".into()); }
            header_seen = true; continue;
        }
        if trimmed == "hang()" { body.push(Stmt::Hang); continue; }
        
        if let Some(args) = trimmed.strip_prefix("poke16(").and_then(|s| s.strip_suffix(')')) {
            let p: Vec<&str> = args.split(',').map(|x| x.trim()).collect();
            let addr = parse_int(p[0])?; let val = parse_int(p[1])?;
            body.push(Stmt::Poke16(addr, val as u16)); continue;
        }
        
        if let Some(args) = trimmed.strip_prefix("poke8(").and_then(|s| s.strip_suffix(')')) {
            let p: Vec<&str> = args.split(',').map(|x| x.trim()).collect();
            let addr = parse_int(p[0])?; let val = parse_int(p[1])?;
            body.push(Stmt::Poke8(addr, val as u8)); continue;
        }
    }
    Ok(body)
}

// 1. BIOS BACKEND: 32-bitový kód
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
                code.push(0xB0); code.push(value); // ZDE BYLA OPRAVA: Odstraněna hvězdička
                code.extend_from_slice(&[0x88, 0x03]);
            }
            Stmt::Hang => code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]),
        }
    }
    code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]);
    code
}

// 2. UEFI BACKEND: 64-bitový kód
fn gen_x86_64(stmts: &[Stmt]) -> Vec<u8> {
    let mut code = Vec::new();
    for stmt in stmts {
        match *stmt {
            Stmt::Poke16(addr, value) => {
                code.extend_from_slice(&[0x48, 0xB8]);
                let addr64 = addr as u64; // ZDE BYLA OPRAVA: Odstraněna hvězdička
                code.extend_from_slice(&addr64.to_le_bytes());
                code.extend_from_slice(&[0x66, 0xB9]); 
                code.extend_from_slice(&value.to_le_bytes());
                code.extend_from_slice(&[0x66, 0x89, 0x08]);
            }
            Stmt::Poke8(addr, value) => {
                code.extend_from_slice(&[0x48, 0xB8]);
                let addr64 = addr as u64; // ZDE BYLA OPRAVA: Odstraněna hvězdička
                code.extend_from_slice(&addr64.to_le_bytes());
                code.push(0xB1); code.push(value); // ZDE BYLA OPRAVA: Odstraněna hvězdička
                code.extend_from_slice(&[0x88, 0x08]);
            }
            Stmt::Hang => code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]),
        }
    }
    // Návrat z UEFI aplikace
    code.extend_from_slice(&[0xB8, 0x00, 0x00, 0x00, 0x00, 0xC3]);
    code
}

fn emit_file(machine_code: Vec<u8>, target: Target, output_file: &str) {
    let (format, arch) = match target {
        Target::Bios32 => (BinaryFormat::Elf, Architecture::I386),
        Target::Uefi64 => (BinaryFormat::Pe, Architecture::X86_64),
    };
    
    let mut obj = Object::new(format, arch, Endianness::Little);
    let text = obj.add_section(vec![], b".text".to_vec(), object::SectionKind::Text);
    let offset = obj.append_section_data(text, &machine_code, 16);

    let name: &[u8] = match target { 
        Target::Bios32 => b"kernel_main".as_slice(), 
        Target::Uefi64 => b"efi_main".as_slice() 
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
    if args.len() != 4 { fail("Použití: pybor <bios|uefi> <vstup.pyb> <výstup>"); }
    
    let target = match args[1].as_str() {
        "bios" => Target::Bios32,
        "uefi" => Target::Uefi64,
        _ => fail("Zvolte buď 'bios' nebo 'uefi'"),
    };
    
    let src = fs::read_to_string(&args[2]).unwrap();
    let ast = parse_program(&src).unwrap();
    
    let mcode = match target {
        Target::Bios32 => gen_x86_32(&ast),
        Target::Uefi64 => gen_x86_64(&ast),
    };
    
    emit_file(mcode, target, &args[3]);
    println!("✅ Kompilace dokončena!");
}
