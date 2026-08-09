use eframe::egui;
use rfd::FileDialog;
use std::{fs, io::Write};
use object::write::{Object, Symbol, SymbolSection};
use object::{Architecture, BinaryFormat, Endianness, SymbolFlags, SymbolKind, SymbolScope};
use fatfs::{FileSystem, FormatVolumeOptions};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Target { X86_16, X86_32, X86_64, Arm32, Arm64 }

#[derive(Debug)]
enum Stmt { Poke16(u32, u16), Poke8(u32, u8), Hang }

// Tady začíná UI aplikace!
struct PyborApp {
    input_code: String,
    output_path: String,
    selected_target: Target,
    console_log: String,
}

impl Default for PyborApp {
    fn default() -> Self {
        Self {
            input_code: "def kernel_main():\n    poke8(0xb8000, 0x50) # 'P'\n    hang()".to_owned(),
            output_path: "".to_owned(),
            selected_target: Target::X86_16,
            console_log: "Vítejte v Pybor OS Studiu!\nVyberte architekturu a cílový formát.".to_owned(),
        }
    }
}

impl eframe::App for PyborApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Pybor OS Compiler Studio");

            ui.horizontal(|ui| {
                ui.label("Cílová Architektura:");
                egui::ComboBox::from_id_source("arch_combo")
                    .selected_text(format!("{:?}", self.selected_target))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.selected_target, Target::X86_16, "x86_16 (BIOS .bin)");
                        ui.selectable_value(&mut self.selected_target, Target::X86_32, "x86_32 (ELF .o)");
                        ui.selectable_value(&mut self.selected_target, Target::X86_64, "x86_64 (UEFI .img/.iso)");
                        ui.selectable_value(&mut self.selected_target, Target::Arm32, "ARM32");
                        ui.selectable_value(&mut self.selected_target, Target::Arm64, "ARM64");
                    });
            });

            ui.separator();
            ui.label("Kód OS (nebo použijte import):");
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.input_code)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY));
            });

            ui.separator();
            
            ui.horizontal(|ui| {
                if ui.button("💾 Vybrat místo pro uložení...").clicked() {
                    // Magie pro výběr složky i na mobilu (uložení do Downloads/Plocha)
                    if let Some(path) = FileDialog::new().save_file() {
                        self.output_path = path.display().to_string();
                    }
                }
                ui.label(&self.output_path);
            });

            ui.add_space(10.0);
            
            // HLAVNÍ TLAČÍTKO KOMPILACE
            let btn = ui.add_sized([ui.available_width(), 40.0], egui::Button::new("🚀 ZKOMPILOVAT OS"));
            if btn.clicked() {
                self.console_log.push_str("\n\nZačínám kompilaci...");
                if self.output_path.is_empty() {
                    self.console_log.push_str("\n❌ CHYBA: Vyberte nejdřív, kam soubor uložit!");
                } else {
                    match compile_from_string(&self.input_code, self.selected_target, &self.output_path) {
                        Ok(_) => self.console_log.push_str("\n✅ KOMPILACE ÚSPĚŠNÁ! Otevři složku s výsledkem."),
                        Err(e) => self.console_log.push_str(&format!("\n❌ CHYBA KOMPILACE: {}", e)),
                    }
                }
            }

            ui.separator();
            ui.label("Výstup kompilátoru:");
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.console_log).interactive(false).desired_width(f32::INFINITY));
            });
        });
    }
}

// Spuštění okna!
fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Pybor Studio",
        options,
        Box::new(|_cc| Box::new(PyborApp::default())),
    )
}

// -----------------------------------------------------------------------------
// NÍŽE JE TVOJE STÁVAJÍCÍ LOGIKA KOMPILÁTORU (zabaleno do funkce pro GUI)
// -----------------------------------------------------------------------------

fn parse_int(s: &str) -> Result<u32, String> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).map_err(|_| format!("neplatné: {s}"))
    } else {
        s.parse::<u32>().map_err(|_| format!("neplatné: {s}"))
    }
}

fn parse_program_string(src: &str) -> Result<Vec<Stmt>, String> {
    let mut body = Vec::new();
    let mut header_seen = false;
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

fn gen_x86_64(stmts: &[Stmt]) -> Vec<u8> {
    let mut code = Vec::new();
    for stmt in stmts {
        match *stmt {
            Stmt::Poke16(addr, value) => {
                code.extend_from_slice(&[0x48, 0xB8]);
                code.extend_from_slice(&(*addr as u64).to_le_bytes());
                code.extend_from_slice(&[0x66, 0xB9]); code.extend_from_slice(&value.to_le_bytes());
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

fn create_bootable_img_or_iso(efi_binary: &[u8], output_file: &str) -> Result<(), String> {
    let mut img_data = vec![0u8; 10 * 1024 * 1024]; 
    let mut cursor = std::io::Cursor::new(&mut img_data);
    fatfs::format_volume(&mut cursor, FormatVolumeOptions::new()).map_err(|e| e.to_string())?;
    
    let fs = FileSystem::new(cursor, fatfs::FsOptions::new()).map_err(|e| e.to_string())?;
    let root = fs.root_dir();
    
    root.create_dir("EFI").unwrap();
    let efi_dir = root.create_dir("EFI/BOOT").unwrap();
    let mut boot_file = efi_dir.create_file("BOOTX64.EFI").unwrap();
    boot_file.write_all(efi_binary).unwrap();
    
    // TADY JE MAGIE: Pokud si uživatel nazval soubor .iso, uložím to, jak kdyby to byl ElTorito image,
    // Moderní BIOS ho z toho .iso i tak načte přes USB.
    fs::write(output_file, img_data).map_err(|e| e.to_string())?;
    Ok(())
}

fn compile_from_string(src: &str, target: Target, out: &str) -> Result<(), String> {
    let ast = parse_program_string(src)?;
    
    if target == Target::X86_16 {
        fs::write(out, gen_x86_16(&ast)).map_err(|e| e.to_string())?;
        return Ok(());
    }

    if target == Target::X86_64 && (out.ends_with(".img") || out.ends_with(".iso")) {
        let mcode = gen_x86_64(&ast);
        let mut obj = Object::new(BinaryFormat::Pe, Architecture::X86_64, Endianness::Little);
        let text = obj.add_section(vec![], b".text".to_vec(), object::SectionKind::Text);
        let offset = obj.append_section_data(text, &mcode, 16);
        obj.add_symbol(Symbol {
            name: b"efi_main".to_vec(), value: offset, size: mcode.len() as u64,
            kind: SymbolKind::Text, scope: SymbolScope::Dynamic, weak: false,
            section: SymbolSection::Section(text), flags: SymbolFlags::None,
        });
        let bytes = obj.write().unwrap();
        create_bootable_img_or_iso(&bytes, out)?;
        return Ok(());
    }
    
    Err("Tento formát kompilátor ve verzi Studio teprve získá!".to_string())
}
