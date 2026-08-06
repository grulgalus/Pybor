use std::{env, fs, process};
use object::write::{Object, Symbol, SymbolSection, SymbolFlags};
use object::{Architecture, BinaryFormat, Endianness, SymbolKind, SymbolScope};

#[derive(Debug, Clone, Copy)]
enum Arch { X86_16, X86_32, X86_64, Arm32, Arm64 }

#[derive(Debug)]
enum Stmt { Poke16(u32, u16), Poke8(u32, u8), Hang }

fn fail(msg: &str) -> ! { eprintln!("{msg}"); process::exit(1); }

fn parse_int(s: &str) -> Result<u32, String> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).map_err(|_| format!("neplatné číslo: {s}"))
    } else {
        s.parse::<u32>().map_err(|_| format!("neplatné číslo: {s}"))
    }
}

fn parse_program(src: &str) -> Result<Vec<Stmt>, String> {
    let mut header_seen = false;
    let mut body = Vec::new();
    for (idx, raw_line) in src.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        
        if !header_seen {
            if trimmed != "def kernel_main():" { return Err(format!("řádek {line_no}: očekávám `def kernel_main():`")); }
            header_seen = true; continue;
        }
        if !raw_line.starts_with("    ") { return Err(format!("řádek {line_no}: chybí odsazení")); }
        if trimmed == "hang()" { body.push(Stmt::Hang); continue; }
        
        if let Some(args) = trimmed.strip_prefix("poke16(").and_then(|s| s.strip_suffix(')')) {
            let parts: Vec<&str> = args.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).collect();
            if parts.len() != 2 { return Err(format!("řádek {line_no}: chybné argumenty")); }
            let addr = parse_int(parts[0])?; let value = parse_int(parts[1])?;
            body.push(Stmt::Poke16(addr, value as u16)); continue;
        }
        
        if let Some(args) = trimmed.strip_prefix("poke8(").and_then(|s| s.strip_suffix(')')) {
            let parts: Vec<&str> = args.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).collect();
            if parts.len() != 2 { return Err(format!("řádek {line_no}: chybné argumenty")); }
            let addr = parse_int(parts[0])?; let value = parse_int(parts[1])?;
            body.push(Stmt::Poke8(addr, value as u8)); continue;
        }
        return Err(format!("řádek {line_no}: neznámý příkaz: {trimmed}"));
    }
    Ok(body)
}

// ZDE JE TO KOUZLO: Emise syrových bajtů pro x86_32 procesor!
fn generate_x86_32_machine_code(stmts: &[Stmt]) -> Vec<u8> {
    let mut code = Vec::new();
    for stmt in stmts {
        match *stmt {
            Stmt::Poke16(addr, value) => {
                // mov ebx, addr (BB nn nn nn nn)
                code.push(0xBB);
                code.extend_from_slice(&addr.to_le_bytes());
                // mov ax, value (66 B8 nn nn)
                code.extend_from_slice(&[0x66, 0xB8]);
                code.extend_from_slice(&value.to_le_bytes());
                // mov [ebx], ax (66 89 03)
                code.extend_from_slice(&[0x66, 0x89, 0x03]);
            }
            Stmt::Poke8(addr, value) => {
                // mov ebx, addr (BB nn nn nn nn)
                code.push(0xBB);
                code.extend_from_slice(&addr.to_le_bytes());
                // mov al, value (B0 nn)
                code.push(0xB0);
                code.push(value);
                // mov [ebx], al (88 03)
                code.extend_from_slice(&[0x88, 0x03]);
            }
            Stmt::Hang => {
                // cli (FA), .hang: hlt (F4), jmp .hang (EB FD)
                code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]);
            }
        }
    }
    // Pojistný hang na konci
    code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]);
    code
}

fn emit_elf(machine_code: Vec<u8>, output_file: &str) {
    let mut obj = Object::new(BinaryFormat::Elf, Architecture::I386, Endianness::Little);
    let text_section = obj.add_section(vec![], b".text".to_vec(), object::SectionKind::Text);
    
    let offset = obj.append_section_data(text_section, &machine_code, 4);

    let symbol = Symbol {
        name: b"kernel_main".to_vec(),
        value: offset,
        size: machine_code.len() as u64,
        kind: SymbolKind::Text,
        scope: SymbolScope::Global,
        weak: false,
        section: SymbolSection::Section(text_section),
        flags: SymbolFlags::None,
    };
    obj.add_symbol(symbol);

    let elf_bytes = obj.write().unwrap();
    fs::write(output_file, elf_bytes).unwrap();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 { fail("Použití: pybor <architektura> <vstup.pyb> <výstup.o>"); }
    
    let arch = match args[1].as_str() {
        "x86_32" => Arch::X86_32,
        _ => fail("Binární kompilátor zatím podporuje pouze real output pro x86_32"),
    };
    
    let src = fs::read_to_string(&args[2]).unwrap_or_else(|e| fail(&format!("nelze číst {}: {}", args[2], e)));
    let ast = parse_program(&src).unwrap_or_else(|e| fail(&format!("chyba parseru: {}", e)));
    
    if let Arch::X86_32 = arch {
        let machine_code = generate_x86_32_machine_code(&ast);
        emit_elf(machine_code, &args[3]);
        println!("✅ Vygenerován skutečný ELF .o soubor!");
    }
}
