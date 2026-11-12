//! SPDX licence expressions: which operands still lack a licence text.
//!
//! `A AND B` needs both texts, `A OR B` one of them. A `WITH` exception is dropped (the licence
//! before it is what needs a text). The old Cargo form `A/B` is read as `A OR B`. An expression
//! that is empty or does not parse is an error: guessing would hide a gap.

use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Id(String),
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

pub fn parse(src: &str) -> Result<Expr, String> {
    let spaced = src
        .replace('(', " ( ")
        .replace(')', " ) ")
        .replace('/', " OR ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    let mut pos = 0;
    if tokens.is_empty() {
        return Err("it is empty".into());
    }
    match or_expr(&tokens, &mut pos) {
        Some(e) if pos == tokens.len() => Ok(e),
        _ => Err("it is not a valid SPDX expression".into()),
    }
}

fn is_op(t: &str, op: &str) -> bool {
    t.eq_ignore_ascii_case(op)
}

fn or_expr(t: &[&str], pos: &mut usize) -> Option<Expr> {
    let mut parts = vec![and_expr(t, pos)?];
    while t.get(*pos).is_some_and(|x| is_op(x, "OR")) {
        *pos += 1;
        parts.push(and_expr(t, pos)?);
    }
    Some(if parts.len() == 1 {
        parts.remove(0)
    } else {
        Expr::Or(parts)
    })
}

fn and_expr(t: &[&str], pos: &mut usize) -> Option<Expr> {
    let mut parts = vec![atom(t, pos)?];
    while t.get(*pos).is_some_and(|x| is_op(x, "AND")) {
        *pos += 1;
        parts.push(atom(t, pos)?);
    }
    Some(if parts.len() == 1 {
        parts.remove(0)
    } else {
        Expr::And(parts)
    })
}

fn atom(t: &[&str], pos: &mut usize) -> Option<Expr> {
    let tok = *t.get(*pos)?;
    *pos += 1;
    if tok == "(" {
        let inner = or_expr(t, pos)?;
        if t.get(*pos) != Some(&")") {
            return None;
        }
        *pos += 1;
        return Some(inner);
    }
    if tok == ")" || is_op(tok, "AND") || is_op(tok, "OR") || is_op(tok, "WITH") {
        return None;
    }
    if t.get(*pos).is_some_and(|x| is_op(x, "WITH")) {
        *pos += 1; // the exception id
        t.get(*pos)?;
        *pos += 1;
    }
    Some(Expr::Id(tok.to_string()))
}

impl Expr {
    /// The ids that must still get a text. `covered` holds lowercase ids that have one.
    pub fn missing(&self, covered: &BTreeSet<String>) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        match self {
            Expr::Id(id) => {
                if !covered.contains(&id.to_ascii_lowercase()) {
                    out.push(id.clone());
                }
            }
            Expr::And(parts) => {
                for p in parts {
                    out.extend(p.missing(covered));
                }
            }
            Expr::Or(parts) => {
                let each: Vec<Vec<String>> = parts.iter().map(|p| p.missing(covered)).collect();
                if each.iter().all(|m| !m.is_empty()) {
                    out.extend(each.into_iter().flatten());
                }
            }
        }
        let mut seen = BTreeSet::new();
        out.retain(|i| seen.insert(i.clone()));
        out
    }
}
