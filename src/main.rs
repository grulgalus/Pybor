use std::{env, fs, process};

#[derive(Debug)]
enum Stmt {
    Poke16(u32, u16),
    Poke8(u32, u8),
    Hang,
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    process::exit(1);
}

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

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if !header_seen {
            if trimmed != "def kernel_main():" {
                return Err(format!("řádek {line_no}: očekávám přesně `def kernel_main():`"));
            }
            header_seen = true;
            continue;
        }

        if !raw_line.starts_with("    ") {
            return Err(format!("řádek {line_no}: tělo funkce musí být odsazené 4 mezerami"));
        }

        if trimmed == "hang()" {
            body.push(Stmt::Hang);
            continue;
        }

        if let Some(args) = trimmed.strip_prefix("poke16(").and_then(|s| s.strip_suffix(')')) {
            let parts: Vec<&str> = args.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).collect();
            if parts.len() != 2 { return Err(format!("řádek {line_no}: poke16() potřebuje 2 argumenty")); }
            let addr = parse_int(parts[0])?;
            let value = parse_int(parts[1])?;
            if value > 0xFFFF { return Err(format!("řádek {line_no}: mimo rozsah u16")); }
            body.push(Stmt::Poke16(addr, value as u16));
            continue;
        }

        if let Some(args) = trimmed.strip_prefix("poke8(").and_then(|s| s.strip_suffix(')')) {
            let parts: Vec<&str> = args.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).collect();
            if parts.len() != 2 { return Err(format!("řádek {line_no}: poke8() potřebuje 2 argumenty")); }
            let addr = parse_int(parts[0])?;
            let value = parse_int(parts[1])?;
            if value > 0xFF { return Err(format!("řádek {line_no}: mimo rozsah u8")); }
            body.push(Stmt::Poke8(addr, value as u8));
            continue;
        }

        return Err(format!("řádek {line_no}: neznámý příkaz: {trimmed}"));
    }

    if !header_seen { return Err("chybí `def kernel_main():`".to_string()); }
    if body.is_empty() { return Err("kernel_main je prázdný".to_string()); }

    Ok(body)
}

fn codegen(stmts: &[Stmt]) -> String {
    let mut out = String::from(
        "bits 32\n\
section .text\n\
global kernel_main\n\n\
kernel_main:\n",
    );

    for stmt in stmts {
        match *stmt {
            Stmt::Poke16(addr, value) => {
                out.push_str(&format!(
                    "    mov ebx, 0x{addr:08x}\n    mov ax, 0x{value:04x}\n    mov word [ebx], ax\n"
                ));
            }
            Stmt::Poke8(addr, value) => {
                out.push_str(&format!(
                    "    mov ebx, 0x{addr:08x}\n    mov al, 0x{value:02x}\n    mov byte [ebx], al\n"
                ));
            }
            Stmt::Hang => {
                out.push_str("    cli\n.hang:\n    hlt\n    jmp .hang\n");
                return out;
            }
        }
    }

    out.push_str("    cli\n.hang:\n    hlt\n    jmp .hang\n");
    out
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 { fail("použití: pybor <vstup.pyb> <výstup.asm>"); }

    let src = fs::read_to_string(&args[1]).unwrap_or_else(|e| fail(&format!("nelze číst {}: {}", args[1], e)));
    let ast = parse_program(&src).unwrap_or_else(|e| fail(&format!("chyba parseru: {}", e)));
    let asm = codegen(&ast);
    fs::write(&args[2], asm).unwrap_or_else(|e| fail(&format!("nelze zapsat {}: {}", args[2], e)));
}
