use eframe::egui;
use rfd::FileDialog;
use std::{fs, io::Write};
use object::write::{Object, Symbol, SymbolSection};
use object::{Architecture, BinaryFormat, Endianness, SymbolFlags, SymbolKind, SymbolScope};
use fatfs::{FileSystem, FormatVolumeOptions};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Target { X86_16, X86_32, X86_64, Arm32, Arm64, Riscv32, Riscv64, Mips, Mips64, PowerPc, PowerPc64, Sparc64, Wasm32 }

#[derive(Debug, Clone, Copy, PartialEq)]
enum Language { English, Czech }

#[derive(Debug)]
enum Stmt { Print(String), Hang }

pub struct PyborApp {
    input_code: String,
    output_path: String,
    selected_target: Target,
    console_log: String,
    language: Language,
}

impl Default for PyborApp {
    fn default() -> Self {
        Self {
            input_code: "def main():\n    print(\"Pybor is compiling...\")\n    hang()".to_owned(),
            output_path: "".to_owned(),
            selected_target: Target::X86_16,
            console_log: "Welcome to Pybor Studio! / Vítejte v Pybor Studiu!".to_owned(),
            language: Language::English,
        }
    }
}

fn t(lang: Language, en: &str, cz: &str) -> String {
    match lang {
        Language::English => en.to_string(),
        Language::Czech => cz.to_string(),
    }
}

impl eframe::App for PyborApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Pybor Studio");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.selectable_value(&mut self.language, Language::Czech, "🇨🇿 CS");
                    ui.selectable_value(&mut self.language, Language::English, "🇬🇧 EN");
                });
            });
            ui.separator();
            
            ui.horizontal(|ui| {
                ui.label(t(self.language, "Architecture:", "Architektura:"));
                egui::ComboBox::from_id_source("arch_combo")
                    .selected_text(format!("{:?}", self.selected_target))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.selected_target, Target::X86_16, "x86 (16-bit Raw .bin)");
                        ui.selectable_value(&mut self.selected_target, Target::X86_32, "x86 (32-bit ELF)");
                        ui.selectable_value(&mut self.selected_target, Target::X86_64, "x86 (64-bit UEFI .img)");
                        ui.selectable_value(&mut self.selected_target, Target::Arm32, "ARM (32-bit ELF)");
                        ui.selectable_value(&mut self.selected_target, Target::Arm64, "ARM (64-bit ELF)");
                        ui.selectable_value(&mut self.selected_target, Target::Riscv32, "RISC-V (32-bit ELF)");
                        ui.selectable_value(&mut self.selected_target, Target::Riscv64, "RISC-V (64-bit ELF)");
                        ui.selectable_value(&mut self.selected_target, Target::Mips, "MIPS (32-bit ELF)");
                        ui.selectable_value(&mut self.selected_target, Target::Mips64, "MIPS (64-bit ELF)");
                        ui.selectable_value(&mut self.selected_target, Target::PowerPc, "PowerPC (32-bit ELF)");
                        ui.selectable_value(&mut self.selected_target, Target::PowerPc64, "PowerPC (64-bit ELF)");
                        ui.selectable_value(&mut self.selected_target, Target::Sparc64, "SPARC (64-bit ELF)");
                        ui.selectable_value(&mut self.selected_target, Target::Wasm32, "WebAssembly (32-bit WASM)");
                    });
            });
            ui.separator();
            
            ui.label(t(self.language, "Source code:", "Zdrojový kód:"));
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.input_code).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
            });
            ui.separator();
            
            ui.horizontal(|ui| {
                if ui.button(t(self.language, "💾 Save as...", "💾 Uložit jako...")).clicked() {
                    if let Some(path) = FileDialog::new().save_file() {
                        self.output_path = path.display().to_string();
                    }
                }
                ui.label(&self.output_path);
            });
            ui.add_space(10.0);
            
            if ui.add_sized([ui.available_width(), 40.0], egui::Button::new(t(self.language, "🚀 COMPILE", "🚀 ZKOMPILOVAT"))).clicked() {
                self.console_log.push_str(&t(self.language, "\n\nStarting compilation...", "\n\nZačínám kompilaci..."));
                if self.output_path.is_empty() {
                    self.console_log.push_str(&t(self.language, "\n❌ ERROR: Select save location first!", "\n❌ CHYBA: Vyberte nejdřív místo uložení!"));
                } else {
                    match compile_from_string(&self.input_code, self.selected_target, &self.output_path) {
                        Ok(_) => self.console_log.push_str(&format!("\n✅ {} {:?} -> {}", t(self.language, "SUCCESS! Generated for", "ÚSPĚCH! Vygenerováno pro"), self.selected_target, self.output_path)),
                        Err(e) => self.console_log.push_str(&format!("\n❌ {}: {}", t(self.language, "COMPILATION ERROR", "CHYBA KOMPILACE"), e)),
                    }
                }
            }
            ui.separator();
            ui.label(t(self.language, "Compiler output:", "Výstup kompilátoru:"));
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut self.console_log).interactive(false).desired_width(f32::INFINITY));
            });
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([700.0, 650.0]),
        ..Default::default()
    };
    eframe::run_native("Pybor Studio", options, Box::new(|_cc| Box::new(PyborApp::default())))
}

fn parse_program_string(src: &str) -> Result<Vec<Stmt>, String> {
    let mut body = Vec::new();
    let mut header_seen = false;
    for raw_line in src.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        if trimmed == "def main():" || trimmed == "def kernel_main():" || trimmed == "def bootloader_main():" { header_seen = true; continue; }
        if !header_seen { continue; }
        if trimmed == "hang()" { body.push(Stmt::Hang); continue; }
        if let Some(text) = trimmed.strip_prefix("print(\"").and_then(|s| s.strip_suffix("\")")) {
            body.push(Stmt::Print(text.to_string())); continue;
        }
    }
    Ok(body)
}

fn gen_x86_16(stmts: &[Stmt]) -> Vec<u8> {
    let mut code = Vec::new();
    code.extend_from_slice(&[0xB8, 0x00, 0xB8, 0x8E, 0xC0]);
    let mut screen_offset: u16 = 0; 
    for stmt in stmts {
        match stmt {
            Stmt::Print(text) => {
                for ch in text.chars() {
                    let ascii = ch as u8;
                    code.push(0xBB); code.extend_from_slice(&screen_offset.to_le_bytes()); 
                    code.extend_from_slice(&[0x26, 0xC6, 0x07, ascii]);                    
                    code.extend_from_slice(&[0x26, 0xC6, 0x47, 0x01, 0x0A]);               
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
                    code.extend_from_slice(&[0x48, 0xB8]); code.extend_from_slice(&(screen_offset as u64).to_le_bytes());
                    code.extend_from_slice(&[0xC6, 0x00, ascii]);
                    code.extend_from_slice(&[0xC6, 0x40, 0x01, 0x0E]);
                    screen_offset += 2;
                }
            }
            Stmt::Hang => code.extend_from_slice(&[0xFA, 0xF4, 0xEB, 0xFD]),
        }
    }
    code.extend_from_slice(&[0xB8, 0x00, 0x00, 0x00, 0x00, 0xC3]);
    code
}

fn gen_dummy(_stmts: &[Stmt]) -> Vec<u8> { vec![0x00, 0x00, 0x00, 0x00] } 

fn create_bootable_img(efi_binary: &[u8], output_file: &str) -> Result<(), String> {
    let mut img_data = vec![0u8; 10 * 1024 * 1024]; 
    
    // TADY JE OPRAVA PRO BORROW CHECKER!
    // Vytvoříme blok, ve kterém FatFS zapisuje do paměti. 
    // Na konci bloku kurzor i systém FatFS zmizí, takže paměť je opět naše!
    {
        let mut cursor = std::io::Cursor::new(&mut img_data);
        fatfs::format_volume(&mut cursor, FormatVolumeOptions::new()).map_err(|e| e.to_string())?;
        
        let fs = FileSystem::new(cursor, fatfs::FsOptions::new()).map_err(|e| e.to_string())?;
        fs.root_dir().create_dir("EFI").unwrap();
        fs.root_dir().create_dir("EFI/BOOT").unwrap().create_file("BOOTX64.EFI").unwrap().write_all(efi_binary).unwrap();
    } // Zde kurzor a fs umírají
    
    // Nyní můžeme bezpečně vzít data a uložit je na disk.
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
    
    let (format, arch, endian) = match target {
        Target::X86_32 => (BinaryFormat::Elf, Architecture::I386, Endianness::Little),
        Target::X86_64 => (BinaryFormat::Elf, Architecture::X86_64, Endianness::Little),
        Target::Arm32 => (BinaryFormat::Elf, Architecture::Arm, Endianness::Little),
        Target::Arm64 => (BinaryFormat::Elf, Architecture::Aarch64, Endianness::Little),
        Target::Riscv32 => (BinaryFormat::Elf, Architecture::Riscv32, Endianness::Little),
        Target::Riscv64 => (BinaryFormat::Elf, Architecture::Riscv64, Endianness::Little),
        Target::Mips => (BinaryFormat::Elf, Architecture::Mips, Endianness::Big),
        Target::Mips64 => (BinaryFormat::Elf, Architecture::Mips64, Endianness::Big),
        Target::PowerPc => (BinaryFormat::Elf, Architecture::PowerPc, Endianness::Big),
        Target::PowerPc64 => (BinaryFormat::Elf, Architecture::PowerPc64, Endianness::Big),
        Target::Sparc64 => (BinaryFormat::Elf, Architecture::Sparc64, Endianness::Big),
        Target::Wasm32 => (BinaryFormat::Wasm, Architecture::Wasm32, Endianness::Little),
        _ => return Err("Unknown target!".to_string()),
    };

    let mcode = gen_dummy(&ast);
    let mut obj = Object::new(format, arch, endian);
    let text = obj.add_section(vec![], b".text".to_vec(), object::SectionKind::Text);
    let offset = obj.append_section_data(text, &mcode, 16);
    obj.add_symbol(Symbol { name: b"main".to_vec(), value: offset, size: mcode.len() as u64, kind: SymbolKind::Text, scope: SymbolScope::Dynamic, weak: false, section: SymbolSection::Section(text), flags: SymbolFlags::None });
    fs::write(out, obj.write().unwrap()).map_err(|e| e.to_string())?;
    Ok(())
}

// TOTO JE VSTUPNÍ BOD POUZE PRO ANDROID MOBILY!
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: eframe::android_activity::AndroidApp) {
    use eframe::{NativeOptions, Renderer};
    
    let mut options = NativeOptions::default();
    options.renderer = Renderer::Glow; // Mobily potřebují Glow vykreslování
    
    eframe::run_native_android_app(
        app,
        "Pybor Studio",
        options,
        Box::new(|_cc| Box::new(PyborApp::default())),
    ).unwrap();
}
