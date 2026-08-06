use std::{env, fs, process};

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
            if value > 0xFFFF { return Err(format!("řádek {line_no}: mimo rozsah")); }
            body.push(Stmt::Poke16(addr, value as u16)); continue;
        }
        
        if let Some(args) = trimmed.strip_prefix("poke8(").and_then(|s| s.strip_suffix(')')) {
            let parts: Vec<&str> = args.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).collect();
            if parts.len() != 2 { return Err(format!("řádek {line_no}: chybné argumenty")); }
            let addr = parse_int(parts[0])?; let value = parse_int(parts[1])?;
            if value > 0xFF { return Err(format!("řádek {line_no}: mimo rozsah")); }
            body.push(Stmt::Poke8(addr, value as u8)); continue;
        }
        return Err(format!("řádek {line_no}: neznámý příkaz: {trimmed}"));
    }
    if !header_seen { return Err("chybí `def kernel_main():`".to_string()); }
    if body.is_empty() { return Err("kernel_main je prázdný".to_string()); }
    Ok(body)
}

fn codegen(stmts: &[Stmt], arch: Arch) -> String {
    let mut out = String::new();
    match arch {
        Arch::X86_16 => out.push_str("bits 16\nsection .text\nglobal kernel_main\n\nkernel_main:\n"),
        Arch::X86_32 => out.push_str("bits 32\nsection .text\nglobal kernel_main\n\nkernel_main:\n"),
        Arch::X86_64 => out.push_str("bits 64\nsection .text\nglobal kernel_main\n\nkernel_main:\n"),
        Arch::Arm32 | Arch::Arm64 => out.push_str(".section .text\n.global kernel_main\n\nkernel_main:\n"),
    }

    for stmt in stmts {
        match *stmt {
            Stmt::Poke16(addr, value) => match arch {
                Arch::X86_16 => out.push_str(&format!("    mov bx, 0x{addr:04x}\n    mov ax, 0x{value:04x}\n    mov [bx], ax\n")),
                Arch::X86_32 => out.push_str(&format!("    mov ebx, 0x{addr:08x}\n    mov ax, 0x{value:04x}\n    mov [ebx], ax\n")),
                Arch::X86_64 => out.push_str(&format!("    mov rbx, 0x{addr:016x}\n    mov ax, 0x{value:04x}\n    mov [rbx], ax\n")),
                Arch::Arm32 => out.push_str(&format!("    ldr r0, =0x{addr:08x}\n    ldr r1, =0x{value:04x}\n    strh r1, [r0]\n")),
                Arch::Arm64 => out.push_str(&format!("    ldr x0, =0x{addr:016x}\n    mov w1, #0x{value:04x}\n    strh w1, [x0]\n")),
            },
            Stmt::Poke8(addr, value) => match arch {
                Arch::X86_16 => out.push_str(&format!("    mov bx, 0x{addr:04x}\n    mov al, 0x{value:02x}\n    mov [bx], al\n")),
                Arch::X86_32 => out.push_str(&format!("    mov ebx, 0x{addr:08x}\n    mov al, 0x{value:02x}\n    mov [ebx], al\n")),
                Arch::X86_64 => out.push_str(&format!("    mov rbx, 0x{addr:016x}\n    mov al, 0x{value:02x}\n    mov [rbx], al\n")),
                Arch::Arm32 => out.push_str(&format!("    ldr r0, =0x{addr:08x}\n    mov r1, #0x{value:02x}\n    strb r1, [r0]\n")),
                Arch::Arm64 => out.push_str(&format!("    ldr x0, =0x{addr:016x}\n    mov w1, #0x{value:02x}\n    strb w1, [x0]\n")),
            },
            Stmt::Hang => match arch {
                Arch::X86_16 | Arch::X86_32 | Arch::X86_64 => { out.push_str("    cli\n.hang:\n    hlt\n    jmp .hang\n"); return out; }
                Arch::Arm32 | Arch::Arm64 => { out.push_str(".hang:\n    b .hang\n"); return out; }
            }
        }
    }
    match arch {
        Arch::X86_16 | Arch::X86_32 | Arch::X86_64 => out.push_str("    cli\n.hang:\n    hlt\n    jmp .hang\n"),
        Arch::Arm32 | Arch::Arm64 => out.push_str(".hang:\n    b .hang\n"),
    }
    out
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 { fail("Použití: pybor <architektura> <vstup.pyb> <výstup.asm>\nArchitektury: x86_16, x86_32, x86_64, arm32, arm64"); }
    
    let arch = match args[1].as_str() {
        "x86_16" => Arch::X86_16, "x86_32" => Arch::X86_32, "x86_64" => Arch::X86_64, "arm32" => Arch::Arm32, "arm64" => Arch::Arm64,
        _ => fail("Neznámá architektura! Zvolte: x86_16, x86_32, x86_64, arm32, arm64"),
    };
    
    let src = fs::read_to_string(&args[2]).unwrap_or_else(|e| fail(&format!("nelze číst {}: {}", args[2], e)));
    let ast = parse_program(&src).unwrap_or_else(|e| fail(&format!("chyba parseru: {}", e)));
    let asm = codegen(&ast, arch);
    fs::write(&args[3], asm).unwrap_or_else(|e| fail(&format!("nelze zapsat {}: {}", args[3], e)));
}
