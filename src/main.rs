use eframe::egui;
use rfd::FileDialog;
use std::{fs, io::Write};
use object::write::{Object, Symbol, SymbolSection};
use object::{Architecture, BinaryFormat, Endianness, SymbolFlags, SymbolKind, SymbolScope};
use fatfs::{FileSystem, FormatVolumeOptions};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Target { X86_16, X86_32, X86_64, Arm32, Arm64 }

#[derive(Debug)]
enum Stmt { 
    Print(String), // Nová Pythonovská nádhera!
    Hang 
}

// ==========================================
// GRAFICKÉ ROZHRANÍ (Pybor Studio)
// ==========================================
struct PyborApp {
    input_code: String,
    output_path: String,
    selected_target: Target,
    console_log: String,
}

impl Default for PyborApp {
    fn default() -> Self {
        Self {
            input_code: "def kernel_main():\n    print(\"Pybor OS nabiha...\")\n    hang()".to_owned(),
            output_path: "".to_owned(),
            selected_target: Target::X86_16,
            console_log: "Vítejte v Pybor OS Studiu!\nNapište kód a vyberte formát pro kompilaci.".to_owned(),
        }
    }
}

impl eframe::App for PyborApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Pybor OS Compiler Studio");
            
            ui.horizontal(|ui| {
                ui.label("Cílový systém:");
                egui::ComboBox::from_id_source("arch_combo")
                    .selected_text(format!("{:?}", self.selected_target))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.selected_target, Target::X86_16, "x86_16 (BIOS .bin)");
                        ui.selectable_value(&mut self.selected_target, Target::X86_32, "x86_32 (ELF Kernel)");
                        ui.selectable_value(&mut self.selected_target, Target::X86_64, "x86_64 (UEFI Boot .img)");
                        ui.selectable_value(&mut self.selected_target, Target::Arm32, "ARM32");
                        ui.selectable_value(&mut self.selected_target, Target::Arm64, "ARM64");
                    });
            });
            ui.separator();
            
            ui.label("Zdrojový kód (Python-like):");
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.input_code).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
            });
            ui.separator();
            
            ui.horizontal(|ui| {
                if ui.button("💾 Uložit jako...").clicked() {
                    if let Some(path) = FileDialog::new().save_file() {
                        self.output_path = path.display().to_string();
                    }
                }
                ui.label(&self.output_path);
            });
            ui.add_space(10.0);
            
            if ui.add_sized([ui.available_width(), 40.0], egui::Button::new("🚀 ZKOMPILOVAT OS")).clicked() {
                self.console_log.push_str("\n\nZačínám kompilaci...");
                if self.output_path.is_empty() {
                    self.console_log.push_str("\n❌ CHYBA: Vyberte místo uložení!");
                } else {
                    match compile_from_string(&self.input_code, self.selected_target, &self.output_path) {
                        Ok(_) => self.console_log.push_str(&format!("\n✅ ÚSPĚCH! Soubor uložen do {}", self.output_path)),
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

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([650.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("Pybor Studio", options, Box::new(|_cc| Box::new(PyborApp::default())))
}

// ==========================================
// PYBOR KOMPILÁTOR - JÁDRO
// ==========================================
fn parse_program_string(src: &str) -> Result<Vec<Stmt>, String> {
    let mut body = Vec::new();
    let mut header_seen = false;
    for raw_line in src.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        
        if trimmed == "def bootloader_main():" || trimmed == "def kernel_main():" { header_seen = true; continue; }
        if !header_seen { continue; }
        
        if trimmed == "hang()" { body.push(Stmt::Hang); continue; }
        
        // Hledá čistý Pythonovský print("Text")
        if let Some(text) = trimmed.strip_prefix("print(\"").and_then(|s| s.strip_suffix("\")")) {
            body.push(Stmt::Print(text.to_string())); continue;
        }
    }
    Ok(body)
}

fn gen_x86_16(stmts: &[Stmt]) -> Vec<u8> {
    let mut code = Vec::new();
    
    // Nastavení ES na 0xB800 (Video RAM segment)
    code.extend_from_slice(&[0xB8, 0x00, 0xB8, 0x8E, 0xC0]);
    let mut screen_offset: u16 = 0; 
    
    for stmt in stmts {
        match stmt {
            Stmt::Print(text) => {
                for ch in text.chars() {
                    let ascii = ch as u8;
                    code.push(0xBB); code.extend_from_slice(&screen_offset.to_le_bytes()); // bx = offset
                    code.extend_from_slice(&[0x26, 0xC6, 0x07, ascii]);                    // es:[bx] = ascii
                    code.extend_from_slice(&[0x26, 0xC6, 0x47, 0x01, 0x0A]);               // es:[bx+1] = color (Zelená!)
                    screen_offset += 2;
                }
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
    let mut screen_offset: u32 = 0xb8000;
    
    for stmt in stmts {
        match stmt {
            Stmt::Print(text) => {
                for ch in text.chars() {
                    let ascii = ch as u8;
                    code.extend_from_slice(&[0x48, 0xB8]); code.extend_from_slice(&(screen_offset as u64).to_le_bytes()); // rax = adresa
                    code.extend_from_slice(&[0xC6, 0x00, ascii]);          // [rax] = ascii
                    code.extend_from_slice(&[0xC6, 0x40, 0x01, 0x0E]);     // [rax+1] = color (Žlutá!)
                    screen_offset += 2;
                }
            }
            Stmt::Hang => code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]),
        }
    }
    code.extend_from_slice(&[0xB8, 0x00, 0x00, 0x00, 0x00, 0xC3]);
    code
}

fn gen_dummy(_stmts: &[Stmt]) -> Vec<u8> { vec![0xFE, 0xE7] } // Běžná zástupná věc pro nepodporované (ARM)

// Tvorba Disku z UEFI binárky!
fn create_bootable_img(efi_binary: &[u8], output_file: &str) -> Result<(), String> {
    let mut img_data = vec![0u8; 10 * 1024 * 1024]; 
    let mut cursor = std::io::Cursor::new(&mut img_data);
    fatfs::format_volume(&mut cursor, FormatVolumeOptions::new()).map_err(|e| e.to_string())?;
    
    let fs = FileSystem::new(cursor, fatfs::FsOptions::new()).map_err(|e| e.to_string())?;
    let root = fs.root_dir();
    root.create_dir("EFI").unwrap();
    root.create_dir("EFI/BOOT").unwrap().create_file("BOOTX64.EFI").unwrap().write_all(efi_binary).unwrap();
    
    fs::write(output_file, img_data).map_err(|e| e.to_string())?;
    Ok(())
}

fn compile_from_string(src: &str, target: Target, out: &str) -> Result<(), String> {
    let ast = parse_program_string(src)?;
    
    if target == Target::X86_16 { fs::write(out, gen_x86_16(&ast)).map_err(|e| e.to_string())?; return Ok(()); }

    if target == Target::X86_64 && out.ends_with(".img") {
        let mcode = gen_x86_64(&ast);
        let mut obj = Object::new(BinaryFormat::Pe, Architecture::X86_64, Endianness::Little);
        let text = obj.add_section(vec![], b".text".to_vec(), object::SectionKind::Text);
        let offset = obj.append_section_data(text, &mcode, 16);
        obj.add_symbol(Symbol { name: b"efi_main".to_vec(), value: offset, size: mcode.len() as u64, kind: SymbolKind::Text, scope: SymbolScope::Dynamic, weak: false, section: SymbolSection::Section(text), flags: SymbolFlags::None });
        let bytes = obj.write().unwrap();
        create_bootable_img(&bytes, out)?;
        return Ok(());
    }
    
    // Pro ostatní formáty jako ARM to uděláme dummy:
    let mcode = gen_dummy(&ast);
    fs::write(out, mcode).map_err(|e| e.to_string())?;
    Ok(())
}
