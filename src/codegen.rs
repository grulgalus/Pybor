use crate::ast::{Expr, Statement};

pub struct CodeGenerator {
    asm: String,
    stack_offset: i32,
    variables: std::collections::HashMap<String, i32>, // Jméno -> offset na stacku
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self {
            asm: String::new(),
            stack_offset: 0,
            variables: std::collections::HashMap::new(),
        }
    }

    pub fn generate(&mut self, ast: &[Statement]) -> String {
        // Hlavička pro x86_64 (Linux/ELF) pro účely testování.
        // Pro skutečný OS by zde byla freestanding hlavička (např. multiboot).
        self.asm.push_str(".global _start\n");
        self.asm.push_str(".text\n");
        self.asm.push_str("_start:\n");
        self.asm.push_str("    push rbp\n");
        self.asm.push_str("    mov rbp, rsp\n");

        for stmt in ast {
            self.gen_statement(stmt);
        }

        // Konec programu (exit syscall pro Linux, aby to nespadlo při testování)
        self.asm.push_str("    mov rax, 60\n"); // syscall: sys_exit
        self.asm.push_str("    mov rdi, 0\n");  // exit code 0
        self.asm.push_str("    syscall\n");

        self.asm.clone()
    }

    fn gen_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Assignment { name, value } => {
                self.gen_expr(value);
                // Výsledek výrazu je v RAX. Uložíme ho na stack.
                self.stack_offset -= 8;
                self.variables.insert(name.clone(), self.stack_offset);
                self.asm.push_str(&format!("    push rax\n")); 
            }
            _ => {}
        }
    }

    fn gen_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Number(val) => {
                self.asm.push_str(&format!("    mov rax, {}\n", val));
            }
            Expr::Identifier(name) => {
                if let Some(offset) = self.variables.get(name) {
                    self.asm.push_str(&format!("    mov rax, QWORD PTR [rbp{}]\n", offset));
                } else {
                    panic!("Nedefinovana promenna: {}", name);
                }
            }
            Expr::BinaryOp(left, op, right) => {
                // Velmi naivní kompilace binární operace pro ukázku
                self.gen_expr(right);
                self.asm.push_str("    push rax\n");
                self.gen_expr(left);
                self.asm.push_str("    pop rbx\n");
                
                match op.as_str() {
                    "+" => self.asm.push_str("    add rax, rbx\n"),
                    "-" => self.asm.push_str("    sub rax, rbx\n"),
                    _ => panic!("Nepodporovany operator"),
                }
            }
        }
    }
}
