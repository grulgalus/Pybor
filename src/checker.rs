use std::collections::HashMap;
use crate::ast::{Expr, Statement};

#[derive(Debug, PartialEq)]
enum OwnershipState {
    Owned,
    Moved,
}

pub struct BorrowChecker {
    scopes: Vec<HashMap<String, OwnershipState>>,
}

impl BorrowChecker {
    pub fn new() -> Self {
        Self { scopes: vec![HashMap::new()] }
    }

    pub fn check_statement(&mut self, stmt: &Statement) -> Result<(), String> {
        match stmt {
            Statement::Assignment { name, value } => {
                self.check_expr(value)?;
                self.scopes.last_mut().unwrap().insert(name.clone(), OwnershipState::Owned);
                Ok(())
            }
            Statement::Return(expr) => self.check_expr(expr),
            _ => Ok(()),
        }
    }

    fn check_expr(&mut self, expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::Identifier(name) => {
                if let Some(state) = self.scopes.last_mut().unwrap().get_mut(name) {
                    if *state == OwnershipState::Moved {
                        return Err(format!("Use of moved value: {}", name));
                    }
                    *state = OwnershipState::Moved;
                    Ok(())
                } else {
                    Err(format!("Undefined variable: {}", name))
                }
            }
            Expr::BinaryOp(left, _, right) => {
                self.check_expr(left)?;
                self.check_expr(right)?;
                Ok(())
            }
            Expr::Number(_) => Ok(()),
        }
    }
}
