// VSTUPNÍ BOD POUZE PRO POČÍTAČE
use pybor::PyborApp; // Načteme celou apku z našeho nového lib.rs

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([700.0, 650.0]),
        ..Default::default()
    };
    eframe::run_native("Pybor Studio", options, Box::new(|_cc| Box::new(PyborApp::default())))
}
