use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use super::assembler::Assembler;
use super::n_compiler::NCompiler;

pub struct ProjectResolver;

impl ProjectResolver {
    pub fn normalize_path(path: &str) -> String {
        let path = path.replace('\\', "/").trim().to_string();
        let path = path.trim_start_matches('/');
        let parts: Vec<&str> = path.split('/').collect();
        let mut list = Vec::new();
        for p in parts {
            if p.is_empty() || p == "." {
                continue;
            }
            if p == ".." {
                if !list.is_empty() && list.last() != Some(&"..") {
                    list.pop();
                }
            } else {
                list.push(p);
            }
        }
        list.join("/")
    }

    pub fn is_assembly_file(path: &str) -> bool {
        let p = path.to_lowercase();
        p.ends_with(".asm") || p.ends_with(".s")
    }

    pub fn is_n_file(path: &str) -> bool {
        path.to_lowercase().ends_with(".n")
    }

    pub fn is_n2_file(path: &str) -> bool {
        path.to_ascii_lowercase().ends_with(".n2")
    }

    pub fn is_supported_source_file(path: &str) -> bool {
        Self::is_n_file(path) || Self::is_n2_file(path) || Self::is_assembly_file(path)
    }

    pub fn get_import_directive_arg(raw_line: &str) -> Option<String> {
        let line = Assembler::strip_comments(raw_line).trim().to_string();
        if line.is_empty() {
            return None;
        }

        if line.starts_with(".include ") || line.starts_with(".include	") {
            return Some(line[9..].trim().to_string());
        }
        if line.starts_with("include ") || line.starts_with("include	") {
            return Some(line[8..].trim().to_string());
        }
        if line.starts_with("#include ") || line.starts_with("#include	") {
            let mut s = line[9..].trim();
            if s.starts_with('<') && s.ends_with('>') && s.len() >= 2 {
                s = &s[1..s.len() - 1].trim();
            }
            return Some(s.to_string());
        }
        if line.starts_with(".import ") || line.starts_with(".import	") {
            return Some(line[8..].trim().to_string());
        }
        if line.starts_with("import ") || line.starts_with("import	") {
            return Some(line[7..].trim().to_string());
        }
        None
    }

    pub fn clean_import_string(raw: &str) -> String {
        let mut s = raw.trim();
        if s.ends_with(';') {
            s = s[..s.len() - 1].trim();
        }
        if (s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\''))
            || (s.starts_with('<') && s.ends_with('>'))
        {
            if s.len() >= 2 {
                s = s[1..s.len() - 1].trim();
            }
        }
        s.to_string()
    }

    pub fn resolve_import_path(
        current_file: &str,
        import_str: &str,
        available_files: &HashSet<String>,
    ) -> Option<String> {
        let clean = Self::clean_import_string(import_str);
        if clean.is_empty() {
            return None;
        }

        let current_dir = if let Some(last_slash) = current_file.rfind('/') {
            &current_file[..last_slash]
        } else {
            ""
        };

        let mut candidates = Vec::new();
        let extensions = ["", ".n", ".asm", ".s", ".S"];

        // 1. Relative to current file's directory
        let rel = if current_dir.is_empty() {
            clean.clone()
        } else {
            format!("{}/{}", current_dir, clean)
        };
        for ext in &extensions {
            candidates.push(Self::normalize_path(&format!("{}{}", rel, ext)));
        }

        // 2. Relative to project root
        for ext in &extensions {
            candidates.push(Self::normalize_path(&format!("{}{}", clean, ext)));
        }

        // 3. Dot notation to slash
        if clean.contains('.')
            && !clean.ends_with(".n")
            && !clean.ends_with(".asm")
            && !clean.ends_with(".s")
            && !clean.ends_with(".S")
        {
            let slash = clean.replace('.', "/");
            for ext in &extensions {
                if !ext.is_empty() {
                    let rel_slash = if current_dir.is_empty() {
                        slash.clone()
                    } else {
                        format!("{}/{}", current_dir, slash)
                    };
                    candidates.push(Self::normalize_path(&format!("{}{}", rel_slash, ext)));
                    candidates.push(Self::normalize_path(&format!("{}{}", slash, ext)));
                }
            }
        }

        // Check exact match
        for cand in &candidates {
            if available_files.contains(cand) {
                return Some(cand.clone());
            }
        }

        // Check case-insensitive match
        for cand in &candidates {
            for file in available_files {
                if file.eq_ignore_ascii_case(cand) {
                    return Some(file.clone());
                }
            }
        }

        None
    }

    pub fn expand_project_files(
        current_file: &str,
        file_map: &HashMap<String, Vec<String>>,
        already_imported: &mut HashSet<String>,
        currently_importing: &mut Vec<String>,
        output_lines: &mut Vec<String>,
        errors: &mut Vec<String>,
        is_entrypoint: bool,
        wrap_counter: &mut usize,
    ) {
        currently_importing.push(current_file.to_string());
        let lines = match file_map.get(current_file) {
            Some(l) => l,
            None => {
                errors.push(format!("Cannot read source lines for file: {}", current_file));
                currently_importing.pop();
                return;
            }
        };

        let wrap_with_jump = !is_entrypoint && Self::is_assembly_file(current_file);
        let mut wrap_id = 0;
        if wrap_with_jump {
            *wrap_counter += 1;
            wrap_id = *wrap_counter;
            output_lines.push(format!("JMP _import_skip_{}", wrap_id));
        }

        let available_files: HashSet<String> = file_map.keys().cloned().collect();

        for (i, raw_line) in lines.iter().enumerate() {
            if let Some(import_arg) = Self::get_import_directive_arg(raw_line) {
                let resolved = Self::resolve_import_path(current_file, &import_arg, &available_files);
                match resolved {
                    None => {
                        errors.push(format!("{}:{}: Cannot resolve import '{}'", current_file, i + 1, import_arg));
                    }
                    Some(res) => {
                        if currently_importing.contains(&res) {
                            errors.push(format!(
                                "{}:{}: Circular import detected: {} -> {}",
                                current_file,
                                i + 1,
                                currently_importing.join(" -> "),
                                res
                            ));
                        } else if !already_imported.contains(&res) {
                            Self::expand_project_files(
                                &res,
                                file_map,
                                already_imported,
                                currently_importing,
                                output_lines,
                                errors,
                                false,
                                wrap_counter,
                            );
                        }
                    }
                }
            } else {
                output_lines.push(raw_line.clone());
            }
        }

        if wrap_with_jump {
            output_lines.push(format!("_import_skip_{}:", wrap_id));
        }

        currently_importing.pop();
        already_imported.insert(current_file.to_string());
    }

    pub fn compile_project(
        files: &HashMap<String, Vec<String>>,
        entrypoint: Option<&str>,
    ) -> Result<Vec<String>, String> {
        if files.is_empty() {
            return Err("Project contains no files!".to_string());
        }

        let mut normalized_map = HashMap::new();
        for (k, v) in files {
            normalized_map.insert(Self::normalize_path(k), v.clone());
        }

        let mut selected_entrypoint: Option<String> = None;
        if let Some(ep) = entrypoint {
            let norm = Self::normalize_path(ep);
            if normalized_map.contains_key(&norm) {
                selected_entrypoint = Some(norm);
            }
        }

        if selected_entrypoint.is_none() {
            let common_entrypoints = [
                "main.n2", "main.n", "main.asm", "main.s", "main.S",
                "entry.n2", "entry.n", "entry.asm", "entry.s", "entry.S",
                "start.n2", "start.asm", "start.s", "start.S", "start.n",
                "index.n2", "index.n", "index.asm", "index.s", "index.S",
            ];
            for cand in &common_entrypoints {
                if normalized_map.contains_key(*cand) {
                    selected_entrypoint = Some(cand.to_string());
                    break;
                }
            }
        }

        if selected_entrypoint.is_none() {
            for path in normalized_map.keys() {
                let lower = path.to_lowercase();
                if lower.ends_with("/main.n2")
                    || lower.ends_with("/entry.n2")
                    || lower.ends_with("/start.n2")
                    || lower.ends_with("/main.n")
                    || lower.ends_with("/main.asm")
                    || lower.ends_with("/main.s")
                    || lower.ends_with("/entry.n")
                    || lower.ends_with("/entry.asm")
                    || lower.ends_with("/entry.s")
                    || lower.ends_with("/start.asm")
                    || lower.ends_with("/start.s")
                {
                    selected_entrypoint = Some(path.clone());
                    break;
                }
            }
        }

        let entry = match selected_entrypoint {
            Some(e) => e,
            None => {
                return Err("Project must contain a main.n2, main.n, main.asm or main.s entrypoint!".to_string());
            }
        };

        if Self::is_n2_file(&entry) {
            return super::n2::compile_project(&normalized_map, &entry);
        }

        let mut expanded_lines = Vec::new();
        let mut errors = Vec::new();
        let mut already_imported = HashSet::new();
        let mut currently_importing = Vec::new();
        let mut wrap_counter = 0;

        Self::expand_project_files(
            &entry,
            &normalized_map,
            &mut already_imported,
            &mut currently_importing,
            &mut expanded_lines,
            &mut errors,
            true,
            &mut wrap_counter,
        );

        if !errors.is_empty() {
            return Err(errors.join("
"));
        }

        NCompiler::compile(&expanded_lines)
    }

    pub fn load_project_from_dir(dir_path: &Path) -> std::io::Result<HashMap<String, Vec<String>>> {
        let mut files = HashMap::new();
        Self::scan_dir_recursive(dir_path, dir_path, &mut files)?;
        Ok(files)
    }

    fn scan_dir_recursive(
        base_dir: &Path,
        current_dir: &Path,
        files: &mut HashMap<String, Vec<String>>,
    ) -> std::io::Result<()> {
        if !current_dir.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(current_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                Self::scan_dir_recursive(base_dir, &path, files)?;
            } else if path.is_file() {
                if let Ok(rel) = path.strip_prefix(base_dir) {
                    let rel_str = rel.to_string_lossy().to_string();
                    if Self::is_supported_source_file(&rel_str) {
                        let content = fs::read_to_string(&path)?;
                        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                        files.insert(Self::normalize_path(&rel_str), lines);
                    }
                }
            }
        }
        Ok(())
    }
}
