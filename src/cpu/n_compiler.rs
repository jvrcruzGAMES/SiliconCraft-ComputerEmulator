use std::collections::HashMap;
use super::assembler::Assembler;
use super::instruction::Opcode;

pub struct NCompiler;

impl NCompiler {
    pub fn compile(source_lines: &[String]) -> Result<Vec<String>, String> {
        let mut assembly = Vec::new();
        let mut var_map: HashMap<String, String> = HashMap::new();
        let mut next_ram_addr = 0;
        let mut label_counter = 1;

        let mut func_stack: Vec<(String, String, bool)> = Vec::new(); // (func_name, end_label, is_interrupt)

        let mut interrupt_handlers = std::collections::HashSet::new();
        for sl in source_lines {
            let stripped = Assembler::strip_comments(sl);
            let l = stripped.trim();
            if l.is_empty() { continue; }
            if l.starts_with("on_") && l.ends_with(')') {
                if let Some(op) = l.find('(') {
                    let h = l[op+1..l.len()-1].trim();
                    if !h.is_empty() { interrupt_handlers.insert(h.to_lowercase()); }
                }
            } else if (l.starts_with("catch(") || l.starts_with("catch ")) && l.ends_with(')') {
                if let Some(op) = l.find('(') {
                    let inner = l[op+1..l.len()-1].trim();
                    let parts: Vec<&str> = inner.split(',').collect();
                    if parts.len() == 2 {
                        interrupt_handlers.insert(parts[1].trim().to_lowercase());
                    }
                }
            } else if let Some(space) = l.find(' ') {
                let first = l[..space].trim().to_uppercase();
                if first == "IVEC" {
                    let rest = l[space+1..].trim();
                    let parts: Vec<&str> = rest.split(',').collect();
                    if parts.len() == 2 {
                        interrupt_handlers.insert(parts[1].trim().to_lowercase());
                    }
                }
            }
        }
        let mut if_stack: Vec<String> = Vec::new(); // end_label
        let mut while_stack: Vec<(String, String)> = Vec::new(); // (start_label, end_label)

        let mut i = 0;
        while i < source_lines.len() {
            let raw = &source_lines[i];
            let mut line = Assembler::strip_comments(raw).trim().to_string();
            if line.is_empty() {
                i += 1;
                continue;
            }

            // Inline assembly block: asm { ... }
            if line == "asm {" || line == "__asm__ {" || line == "assembly {" {
                i += 1;
                while i < source_lines.len() {
                    let raw_asm = &source_lines[i];
                    let clean_asm = Assembler::strip_comments(raw_asm).trim().to_string();
                    if clean_asm == "}" {
                        break;
                    }
                    if !clean_asm.is_empty() {
                        assembly.push(clean_asm);
                    }
                    i += 1;
                }
                i += 1;
                continue;
            }

            // Single line inline assembly: asm("...")
            if line.starts_with("asm(") && line.ends_with(')') {
                let inner = line[4..line.len() - 1].trim();
                let code = if (inner.starts_with('\"') && inner.ends_with('\"')) || (inner.starts_with('\'') && inner.ends_with('\'')) {
                    if inner.len() >= 2 { &inner[1..inner.len() - 1] } else { inner }
                } else {
                    inner
                };
                if !code.trim().is_empty() {
                    assembly.push(code.trim().to_string());
                }
                i += 1;
                continue;
            }

            // Closing brace: }
            if line == "}" {
                if let Some((_fname, end_lbl, is_interrupt)) = func_stack.pop() {
                    let last_str = assembly.last().map(|s| s.as_str());
                    if last_str != Some("RET") && last_str != Some("IRET") && last_str != Some("HLT") {
                        if is_interrupt {
                            assembly.push("IRET".to_string());
                        } else {
                            assembly.push("RET".to_string());
                        }
                    }
                    assembly.push(format!("{}:", end_lbl));
                } else if let Some((start_lbl, end_lbl)) = while_stack.pop() {
                    assembly.push(format!("JMP {}", start_lbl));
                    assembly.push(format!("{}:", end_lbl));
                } else if let Some(end_lbl) = if_stack.pop() {
                    assembly.push(format!("{}:", end_lbl));
                }
                i += 1;
                continue;
            }

            // Label declaration: "label:" or "label: MOV A, 5"
            if let Some(colon) = Assembler::find_label_colon(&line) {
                let lbl = line[..colon + 1].trim();
                let rest = line[colon + 1..].trim();
                if !lbl.contains(' ') && lbl.len() > 1 {
                    assembly.push(lbl.to_string());
                    if rest.is_empty() {
                        i += 1;
                        continue;
                    }
                    line = rest.to_string();
                }
            }

            // Raw Assembly instruction or directive check
            let first_word = line.split_whitespace().next().unwrap_or("").to_uppercase();
            if Opcode::from_str(&first_word) != Opcode::UNKNOWN || matches!(first_word.as_str(), "ORG" | "DB" | "DW" | "DL" | "INCBIN") {
                assembly.push(line.clone());
                i += 1;
                continue;
            }

            // Function declaration: fn name(...) { or func or function
            if (line.starts_with("fn ") || line.starts_with("func ") || line.starts_with("function ") || line.starts_with("def ")) && line.ends_with('{') {
                let start_idx = if line.starts_with("fn ") { 3 } else if line.starts_with("func ") { 5 } else if line.starts_with("def ") { 4 } else { 9 };
                let rest = &line[start_idx..line.len() - 1].trim();
                let func_name = if let Some(paren) = rest.find('(') {
                    rest[..paren].trim()
                } else {
                    rest.trim()
                };

                let end_label = format!("_func_end_{}", label_counter);
                label_counter += 1;

                assembly.push(format!("JMP {}", end_label));
                assembly.push(format!("{}:", func_name));
                let is_interrupt = interrupt_handlers.contains(&func_name.to_lowercase());
                func_stack.push((func_name.to_string(), end_label, is_interrupt));
                i += 1;
                continue;
            }

            // Return statement: return or return expr
            if line == "return" || line.starts_with("return ") {
                if func_stack.is_empty() {
                    assembly.push("HLT".to_string());
                } else if func_stack.last().map(|f| f.2).unwrap_or(false) {
                    assembly.push("IRET".to_string());
                } else {
                    let expr = if line.starts_with("return ") { line[7..].trim() } else { "" };
                    if !expr.is_empty() {
                        Self::compile_assignment("A", expr, &var_map, &mut assembly)?;
                    }
                    assembly.push("RET".to_string());
                }
                i += 1;
                continue;
            }

            // Print / Println: println "Hello" or println A or println(x)
            if line.starts_with("println ") || line.starts_with("println(") || line == "println" || line == "println()" {
                let arg = Self::extract_arg(&line, "println");
                Self::compile_print(&arg, true, &var_map, &mut assembly)?;
                i += 1;
                continue;
            }
            if line.starts_with("print ") || line.starts_with("print(") || line == "print" || line == "print()" {
                let arg = Self::extract_arg(&line, "print");
                Self::compile_print(&arg, false, &var_map, &mut assembly)?;
                i += 1;
                continue;
            }

            // Call function: call(func) or call func or func()
            if (line.starts_with("call(") || line.starts_with("call ")) && line.ends_with(')') {
                let inner = Self::extract_inner_parens(&line);
                assembly.push(format!("CALL {}", inner.trim()));
                i += 1;
                continue;
            }
            if line.ends_with(')') && line.contains('(') && !line.starts_with("if") && !line.starts_with("while") {
                if let Some(open) = line.find('(') {
                    let fname = line[..open].trim();
                    if !fname.is_empty() && !fname.contains(' ') {
                        assembly.push(format!("CALL {}", fname));
                        i += 1;
                        continue;
                    }
                }
            }

            // Variable declaration: var x = expr
            if line.starts_with("var ") {
                let rest = line[4..].trim();
                if let Some(eq) = rest.find('=') {
                    let var_name = rest[..eq].trim().to_lowercase();
                    let expr = rest[eq + 1..].trim();
                    let addr = next_ram_addr;
                    next_ram_addr += 1;
                    let loc = format!("[{}]", addr);
                    var_map.insert(var_name.clone(), loc.clone());
                    Self::compile_assignment(&loc, expr, &var_map, &mut assembly)?;
                } else {
                    let var_name = rest.trim().to_lowercase();
                    let addr = next_ram_addr;
                    next_ram_addr += 1;
                    let loc = format!("[{}]", addr);
                    var_map.insert(var_name, loc.clone());
                    assembly.push(format!("MOV {}, 0", loc));
                }
                i += 1;
                continue;
            }

            // Assignment: target = expr
            if let Some(eq) = line.find('=') {
                if !line.starts_with("==") && (eq == 0 || line.chars().nth(eq - 1) != Some('!')) {
                    let target = line[..eq].trim();
                    let expr = line[eq + 1..].trim();
                    Self::compile_assignment(target, expr, &var_map, &mut assembly)?;
                    i += 1;
                    continue;
                }
            }

            i += 1;
        }

        Ok(assembly)
    }

    fn extract_arg(line: &str, keyword: &str) -> String {
        let rest = line[keyword.len()..].trim();
        if rest.starts_with('(') && rest.ends_with(')') {
            rest[1..rest.len() - 1].trim().to_string()
        } else {
            rest.to_string()
        }
    }

    fn extract_inner_parens(line: &str) -> String {
        if let Some(open) = line.find('(') {
            if let Some(close) = line.rfind(')') {
                return line[open + 1..close].trim().to_string();
            }
        }
        line.to_string()
    }

    fn compile_print(arg: &str, newline: bool, var_map: &HashMap<String, String>, assembly: &mut Vec<String>) -> Result<(), String> {
        let trimmed = arg.trim();
        if trimmed.starts_with('\"') && trimmed.ends_with('\"') {
            assembly.push(format!("OUT {}", trimmed));
        } else if !trimmed.is_empty() {
            let loc = Self::resolve_loc(trimmed, var_map);
            assembly.push(format!("OUT {}", loc));
        }
        if newline {
            assembly.push("OUT 10".to_string());
        }
        Ok(())
    }

    fn compile_assignment(target: &str, expr: &str, var_map: &HashMap<String, String>, assembly: &mut Vec<String>) -> Result<(), String> {
        let target_loc = Self::resolve_loc(target, var_map);
        let trimmed_expr = expr.trim();

        // Check for binary operations: e.g. A * 2 or A + B or A - 5
        for op in ["+", "-", "*", "/", "%", "&", "|", "^"] {
            if let Some(idx) = trimmed_expr.find(op) {
                let left = trimmed_expr[..idx].trim();
                let right = trimmed_expr[idx + op.len()..].trim();
                if !left.is_empty() && !right.is_empty() {
                    let asm_op = match op {
                        "+" => "ADD",
                        "-" => "SUB",
                        "*" => "MUL",
                        "/" => "DIV",
                        "%" => "MOD",
                        "&" => "AND",
                        "|" => "OR",
                        "^" => "XOR",
                        _ => unreachable!(),
                    };
                    let left_loc = Self::resolve_loc(left, var_map);
                    let right_loc = Self::resolve_loc(right, var_map);

                    if target_loc == left_loc {
                        assembly.push(format!("{} {}, {}", asm_op, target_loc, right_loc));
                    } else {
                        assembly.push(format!("MOV {}, {}", target_loc, left_loc));
                        assembly.push(format!("{} {}, {}", asm_op, target_loc, right_loc));
                    }
                    return Ok(());
                }
            }
        }

        let expr_loc = Self::resolve_loc(trimmed_expr, var_map);
        assembly.push(format!("MOV {}, {}", target_loc, expr_loc));
        Ok(())
    }

    fn resolve_loc(token: &str, var_map: &HashMap<String, String>) -> String {
        let lower = token.to_lowercase();
        if let Some(loc) = var_map.get(&lower) {
            loc.clone()
        } else {
            token.to_string()
        }
    }
}
