use std::fmt;

#[derive(Clone, Debug, Default)]
pub struct HyprlandConfig {
    lines: Vec<String>,
    trailing_newline: bool,
}

pub fn parse_config(input: &str) -> HyprlandConfig {
    HyprlandConfig {
        lines: input.lines().map(|line| line.to_string()).collect(),
        trailing_newline: input.ends_with('\n'),
    }
}

impl HyprlandConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn parse_color(&self, value: &str) -> Option<(f64, f64, f64, f64)> {
        parse_color_value(value)
    }

    pub fn add_entry(&mut self, section: &str, entry: &str) {
        let Some((key, value)) = entry.split_once('=') else {
            return;
        };

        let key = key.trim();
        let value = value.trim();
        if key.is_empty() {
            return;
        }

        if let Some((start, end, indent)) = find_section_bounds(&self.lines, section) {
            if let Some(existing_key_line) = find_key_in_section(&self.lines, start, end, key) {
                self.lines[existing_key_line] = format!("{}{} = {}", indent, key, value);
                self.trailing_newline = true;
                return;
            }

            let insert_at = find_insertion_index(&self.lines, start, end);
            self.lines.insert(insert_at, format!("{}{} = {}", indent, key, value));
            self.trailing_newline = true;
            return;
        }

        self.lines.push(format!("{} {{", section));
        self.lines.push(format!("    {} = {}", key, value));
        self.lines.push("}".to_string());
        self.trailing_newline = true;
    }
}

impl fmt::Display for HyprlandConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx, line) in self.lines.iter().enumerate() {
            if idx > 0 {
                f.write_str("\n")?;
            }
            f.write_str(line)?;
        }

        if self.trailing_newline && !self.lines.is_empty() {
            f.write_str("\n")?;
        }

        Ok(())
    }
}

fn parse_color_value(value: &str) -> Option<(f64, f64, f64, f64)> {
    let trimmed = value.trim();
    let stripped = trimmed
        .strip_prefix("rgba(")
        .and_then(|rest| rest.strip_suffix(')'))
        .or_else(|| trimmed.strip_prefix("rgb(").and_then(|rest| rest.strip_suffix(')')));

    if let Some(body) = stripped {
        let hex = body.trim();
        return parse_hyprland_hex_color(hex);
    }

    if let Some(hex) = trimmed.strip_prefix('#') {
        return parse_rgba_hex(hex);
    }

    None
}

fn parse_hyprland_hex_color(value: &str) -> Option<(f64, f64, f64, f64)> {
    match value.len() {
        6 => parse_rgb_hex(value).map(|(r, g, b)| (r, g, b, 1.0)),
        8 => parse_rgba_hex(value),
        _ => None,
    }
}

fn parse_rgb_hex(value: &str) -> Option<(f64, f64, f64)> {
    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some((to_unit(r), to_unit(g), to_unit(b)))
}

fn parse_rgba_hex(value: &str) -> Option<(f64, f64, f64, f64)> {
    if value.len() != 8 {
        return None;
    }

    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;
    let a = u8::from_str_radix(&value[6..8], 16).ok()?;
    Some((to_unit(r), to_unit(g), to_unit(b), to_unit(a)))
}

fn to_unit(component: u8) -> f64 {
    f64::from(component) / 255.0
}

fn find_section_bounds(lines: &[String], section: &str) -> Option<(usize, usize, String)> {
    let section_trimmed = section.trim();
    let mut idx = 0usize;

    while idx < lines.len() {
        let line = lines[idx].trim_start();
        if let Some(name) = parse_section_name(line) {
            if name == section_trimmed {
                let indent = lines[idx]
                    .chars()
                    .take_while(|ch| ch.is_whitespace())
                    .collect::<String>()
                    + "    ";
                let mut depth = 0usize;

                for end in idx + 1..lines.len() {
                    let candidate = lines[end].trim();
                    if candidate.ends_with('{') {
                        depth += 1;
                    }
                    if candidate == "}" {
                        if depth == 0 {
                            return Some((idx + 1, end, indent));
                        }
                        depth -= 1;
                    }
                }

                return Some((idx + 1, lines.len(), indent));
            }
        }
        idx += 1;
    }

    None
}

fn parse_section_name(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let without_brace = trimmed.strip_suffix('{')?.trim_end();
    if without_brace.is_empty() {
        return None;
    }
    Some(without_brace)
}

fn find_key_in_section(
    lines: &[String],
    start: usize,
    end: usize,
    key: &str,
) -> Option<usize> {
    for (idx, line) in lines[start..end].iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&format!("{} =", key)) {
            return Some(start + idx);
        }
    }
    None
}

fn find_insertion_index(lines: &[String], start: usize, end: usize) -> usize {
    let mut insert_at = end;
    for idx in start..end {
        if !lines[idx].trim().is_empty() && lines[idx].trim() != "}" {
            insert_at = idx + 1;
        }
    }
    insert_at
}
