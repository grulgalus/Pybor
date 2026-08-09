use std::{env, fs, io::{Write, Cursor}, process};
use object::write::{Object, Symbol, SymbolSection};
use object::{Architecture, BinaryFormat, Endianness, SymbolFlags, SymbolKind, SymbolScope};
use fatfs::{FileSystem, FormatVolumeOptions};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Target { X86_16, X86_32, X86_64, Arm32, Arm64 }

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

// Detekce závislostí: Hledá klíčové slovo `import "soubor.pyb"`
fn parse_program(main_file: &str) -> Result<Vec<Stmt>, String> {
    let mut body = Vec::new();
    let src = fs::read_to_string(main_file).map_err(|_| format!("Nelze číst soubor {main_file}"))?;
    
    let mut header_seen = false;
    for raw_line in src.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        
        // Zpracování importů!
        if let Some(import_file) = trimmed.strip_prefix("import \"").and_then(|s| s.strip_suffix('"')) {
            println!("Pybor: Objevena závislost -> {}", import_file);
            let mut imported_stmts = parse_program(import_file)?;
            body.append(&mut imported_stmts);
            continue;
        }

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

fn gen_x86_16(stmts: &[Stmt]) -> Vec<u8> {
    let mut code = Vec::new();
    for stmt in stmts {
        match *stmt {
            Stmt::Poke16(addr, value) => {
                code.push(0xBB); code.extend_from_slice(&(addr as u16).to_le_bytes());
                code.push(0xB8); code.extend_from_slice(&value.to_le_bytes());
                code.extend_from_slice(&[0x89, 0x07]);
            }
            Stmt::Poke8(addr, value) => {
                code.push(0xBB); code.extend_from_slice(&(addr as u16).to_le_bytes());
                code.push(0xB0); code.push(*value);
                code.extend_from_slice(&[0x88, 0x07]);
            }
            Stmt::Hang => code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]),
        }
    }
    code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]);
    let mut boot_sector = vec![0; 512];
    let copy_len = std::cmp::min(code.len(), 510);
    boot_sector[..copy_len].copy_from_slice(&code[..copy_len]);
    boot_sector[510] = 0x55; boot_sector[511] = 0xAA;
    boot_sector
}

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

fn gen_x86_64(stmts: &[Stmt]) -> Vec<u8> {
    let mut code = Vec::new();
    for stmt in stmts {
        match *stmt {
            Stmt::Poke16(addr, value) => {
                code.extend_from_slice(&[0x48, 0xB8]);
                let addr64 = *addr as u64;
                code.extend_from_slice(&addr64.to_le_bytes());
                code.extend_from_slice(&[0x66, 0xB9]); code.extend_from_slice(&value.to_le_bytes());
                code.extend_from_slice(&[0x66, 0x89, 0x08]);
            }
            Stmt::Poke8(addr, value) => {
                code.extend_from_slice(&[0x48, 0xB8]);
                let addr64 = *addr as u64;
                code.extend_from_slice(&addr64.to_le_bytes());
                code.push(0xB1); code.push(*value);
                code.extend_from_slice(&[0x88, 0x08]);
            }
            Stmt::Hang => code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]),
        }
    }
    code.extend_from_slice(&[0xB8, 0x00, 0x00, 0x00, 0x00, 0xC3]);
    code
}

fn gen_arm(_stmts: &[Stmt]) -> Vec<u8> { vec![0xFE, 0xE7] }

// Nová funkce pro vytvoření hotového Bootovatelného .IMG disku!
fn create_bootable_img(efi_binary: &[u8], output_file: &str) {
    println!("Pybor: Vytvářím 10MB FAT32 diskový obraz...");
    
    // Vytvoříme v paměti 10MB soubor zaplněný nulami
    let mut img_data = vec![0u8; 10 * 1024 * 1024]; 
    
    // Naformátujeme ten soubor jako FAT (jako by to byla flashka)
    let mut cursor = std::io::Cursor::new(&mut img_data);
    fatfs::format_volume(&mut cursor, FormatVolumeOptions::new()).unwrap();
    
    // Otevřeme ho
    let fs = FileSystem::new(cursor, fatfs::FsOptions::new()).unwrap();
    let root = fs.root_dir();
    
    // Vytvoříme UEFI adresářovou strukturu
    root.create_dir("EFI").unwrap();
    let efi_dir = root.create_dir("EFI/BOOT").unwrap();
    
    // Vložíme do disku tvůj kernel
    let mut boot_file = efi_dir.create_file("BOOTX64.EFI").unwrap();
    boot_file.write_all(efi_binary).unwrap();
    
    // Uložíme to celé na skutečný disk jako .IMG
    fs::write(output_file, img_data).unwrap();
    println!("✅ Hotový .IMG obraz byl uložen do: {}", output_file);
}

fn emit_file(machine_code: Vec<u8>, target: Target, output_file: &str) {
    if target == Target::X86_16 {
        fs::write(output_file, machine_code).unwrap(); return;
    }

    let (format, arch) = match target {
        Target::X86_32 => (BinaryFormat::Elf, Architecture::I386),
        Target::X86_64 => (BinaryFormat::Pe, Architecture::X86_64), // Tady generujeme rovnou PE bez linkování pro interní IMG
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
        name: name.to_vec(), value: offset, size: machine_code.len() as u64,
        kind: SymbolKind::Text, scope: SymbolScope::Dynamic, weak: false,
        section: SymbolSection::Section(text), flags: SymbolFlags::None,
    });

    let bytes = obj.write().unwrap();
    
    // Pokud chce uživatel .IMG, vytvoříme celý disk!
    if output_file.ends_with(".img") && target == Target::X86_64 {
        create_bootable_img(&bytes, output_file);
    } else {
        fs::write(output_file, bytes).unwrap();
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 { fail("Použití: pybor <arch> <vstup.pyb> <výstup(.img/.elf/.o)>"); }
    
    let target = match args[1].as_str() {
        "x86_16" => Target::X86_16, "x86_32" => Target::X86_32, "x86_64" => Target::X86_64,
        "arm32" => Target::Arm32, "arm64" => Target::Arm64, _ => fail("Neznámá architektura!"),
    };
    
    // Tady proběhne magická detekce závislostí a složení kódu!
    let ast = parse_program(&args[2]).unwrap();
    
    let mcode = match target {
        Target::X86_16 => gen_x86_16(&ast), Target::X86_32 => gen_x86_32(&ast),
        Target::X86_64 => gen_x86_64(&ast), Target::Arm32 | Target::Arm64 => gen_arm(&ast),
    };
    
    emit_file(mcode, target, &args[3]);
}
