use std::collections::HashMap;
use std::fs;
use std::path::Path;
use super::instruction::{Operand, Register};
use crate::hardware::Ram;

pub struct Assembler;

impl Assembler {
    pub fn find_label_colon(line: &str) -> Option<usize> {
        let colon = line.find(':')?;
        let space = line.find(' ');
        if let Some(s) = space {
            if colon > s {
                return None;
            }
        }
        let q1 = line.find('\"');
        let q2 = line.find('\'');
        if let Some(q) = q1 {
            if colon > q {
                return None;
            }
        }
        if let Some(q) = q2 {
            if colon > q {
                return None;
            }
        }
        Some(colon)
    }

    pub fn strip_comments(raw: &str) -> String {
        let mut in_quotes = false;
        let mut quote_char = ' ';
        let chars: Vec<char> = raw.chars().collect();
        for i in 0..chars.len() {
            let c = chars[i];
            if (c == '\"' || c == '\'') && (i == 0 || chars[i - 1] != '\\') {
                if in_quotes && c == quote_char {
                    in_quotes = false;
                } else if !in_quotes {
                    in_quotes = true;
                    quote_char = c;
                }
            }
            if !in_quotes {
                if c == ';' || c == '#' {
                    return raw[..i].to_string();
                }
                if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
                    return raw[..i].to_string();
                }
            }
        }
        raw.to_string()
    }

    pub fn split_args(args: &str, limit: usize) -> Vec<String> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut quote_char = ' ';
        let mut paren_depth = 0;
        let mut bracket_depth = 0;

        let chars: Vec<char> = args.chars().collect();
        for i in 0..chars.len() {
            let c = chars[i];
            if (c == '\"' || c == '\'') && (i == 0 || chars[i - 1] != '\\') {
                if in_quotes && c == quote_char {
                    in_quotes = false;
                } else if !in_quotes {
                    in_quotes = true;
                    quote_char = c;
                }
            }
            if !in_quotes {
                if c == '(' { paren_depth += 1; }
                else if c == ')' { paren_depth = (paren_depth - 1).max(0); }
                else if c == '[' { bracket_depth += 1; }
                else if c == ']' { bracket_depth = (bracket_depth - 1).max(0); }
            }

            if c == ',' && !in_quotes && paren_depth == 0 && bracket_depth == 0 && parts.len() + 1 < limit {
                parts.push(current.trim().to_string());
                current.clear();
            } else {
                current.push(c);
            }
        }
        if !current.trim().is_empty() {
            parts.push(current.trim().to_string());
        }
        parts
    }

    pub fn parse_literal(token: &str, labels: &HashMap<String, usize>) -> Result<i32, String> {
        let s = token.trim();
        let lower = s.to_lowercase();
        if let Some(&addr) = labels.get(&lower) {
            return Ok(addr as i32);
        }

        if s.starts_with("0x") || s.starts_with("0X") {
            i64::from_str_radix(&s[2..], 16)
                .map(|v| v as i32)
                .map_err(|e| e.to_string())
        } else if s.starts_with("-0x") || s.starts_with("-0X") {
            i64::from_str_radix(&s[3..], 16)
                .map(|v| -(v as i32))
                .map_err(|e| e.to_string())
        } else if s.starts_with('$') {
            i64::from_str_radix(&s[1..], 16)
                .map(|v| v as i32)
                .map_err(|e| e.to_string())
        } else if s.starts_with("-$") {
            i64::from_str_radix(&s[2..], 16)
                .map(|v| -(v as i32))
                .map_err(|e| e.to_string())
        } else if s.starts_with("0b") || s.starts_with("0B") {
            i64::from_str_radix(&s[2..], 2)
                .map(|v| v as i32)
                .map_err(|e| e.to_string())
        } else if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 3 {
            let inner = &s[1..s.len() - 1];
            if inner.starts_with('\\') && inner.len() == 2 {
                match inner.chars().nth(1) {
                    Some('n') => Ok('\n' as i32),
                    Some('t') => Ok('\t' as i32),
                    Some('r') => Ok('\r' as i32),
                    Some('0') => Ok(0),
                    Some(c) => Ok(c as i32),
                    None => Ok(0),
                }
            } else {
                Ok(inner.chars().next().unwrap_or('\0') as i32)
            }
        } else {
            s.parse::<i32>().map_err(|e| e.to_string())
        }
    }

    pub fn parse_operand(token: &str, labels: &HashMap<String, usize>) -> Operand {
        let t = token.trim();
        if let Some(reg) = Register::from_str(t) {
            return Operand::Reg(reg);
        }
        if t.starts_with('[') && t.ends_with(']') {
            let inner = t[1..t.len() - 1].trim();
            if let Some(inner_reg) = Register::from_str(inner) {
                return Operand::RamReg(inner_reg);
            }
            if let Ok(imm) = Self::parse_literal(inner, labels) {
                if imm >= 0 {
                    return Operand::RamImm(imm as usize);
                }
            }
            return Operand::Dynamic(t.to_string());
        }
        if let Ok(imm) = Self::parse_literal(t, labels) {
            return Operand::Imm(imm);
        }
        Operand::Dynamic(t.to_string())
    }

    pub fn load_static_data(
        lines: &[String],
        ram: &mut Ram,
        labels: &mut HashMap<String, usize>,
        data_labels: &mut HashMap<String, usize>,
        base_dir: Option<&Path>,
    ) {
        let mut current_addr: usize = 0;
        let mut pending_label: Option<String> = None;

        for line_raw in lines {
            let clean = Self::strip_comments(line_raw).trim().to_string();
            if clean.is_empty() {
                continue;
            }

            let mut rest = clean.as_str();
            if let Some(colon) = Self::find_label_colon(&clean) {
                let lbl = clean[..colon].trim().to_lowercase();
                pending_label = Some(lbl);
                rest = clean[colon + 1..].trim();
                if rest.is_empty() {
                    continue;
                }
            }

            let space = rest.find(' ');
            let opcode = space.map(|s| &rest[..s]).unwrap_or(rest).to_uppercase();
            let args = space.map(|s| rest[s + 1..].trim()).unwrap_or("");

            match opcode.as_str() {
                "ORG" => {
                    if let Ok(addr) = Self::parse_literal(args, labels) {
                        current_addr = (addr as usize).min(ram.len());
                        if let Some(lbl) = pending_label.take() {
                            labels.insert(lbl.clone(), current_addr);
                            data_labels.insert(lbl, current_addr);
                        }
                    }
                }
                "DB" => {
                    if let Some(lbl) = pending_label.take() {
                        labels.insert(lbl.clone(), current_addr);
                        data_labels.insert(lbl, current_addr);
                    }
                    let parts = Self::split_args(args, 1000);
                    for p in parts {
                        let trimmed = p.trim();
                        if (trimmed.starts_with('\"') && trimmed.ends_with('\"') && trimmed.len() >= 2)
                            || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
                        {
                            let s = &trimmed[1..trimmed.len() - 1];
                            for b in s.bytes() {
                                let _ = ram.write(current_addr, b as i32);
                                current_addr += 1;
                            }
                        } else if let Ok(val) = Self::parse_literal(trimmed, labels) {
                            let _ = ram.write(current_addr, val & 0xFF);
                            current_addr += 1;
                        }
                    }
                }
                "DW" => {
                    if let Some(lbl) = pending_label.take() {
                        labels.insert(lbl.clone(), current_addr);
                        data_labels.insert(lbl, current_addr);
                    }
                    let parts = Self::split_args(args, 1000);
                    for p in parts {
                        if let Ok(val) = Self::parse_literal(p.trim(), labels) {
                            let _ = ram.write(current_addr, val & 0xFFFF);
                            current_addr += 1;
                        }
                    }
                }
                "DL" => {
                    if let Some(lbl) = pending_label.take() {
                        labels.insert(lbl.clone(), current_addr);
                        data_labels.insert(lbl, current_addr);
                    }
                    let parts = Self::split_args(args, 1000);
                    for p in parts {
                        if let Ok(val) = Self::parse_literal(p.trim(), labels) {
                            let _ = ram.write(current_addr, val);
                            current_addr += 1;
                        }
                    }
                }
                "INCBIN" => {
                    if let Some(lbl) = pending_label.take() {
                        labels.insert(lbl.clone(), current_addr);
                        data_labels.insert(lbl, current_addr);
                    }
                    let filename = args.trim().trim_matches('\"').trim_matches('\'');
                    let path = if let Some(base) = base_dir {
                        base.join(filename)
                    } else {
                        Path::new(filename).to_path_buf()
                    };
                    if let Ok(bytes) = fs::read(&path) {
                        for b in bytes {
                            let _ = ram.write(current_addr, b as i32);
                            current_addr += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
