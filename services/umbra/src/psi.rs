//! PSI reader. The kernel publishes pressure in `/proc/pressure/*`.
//! A test can point the daemon at a stand-in file with the same text format.

#![forbid(unsafe_code)]

use std::fs;
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct PsiSnapshot {
    pub memory_some_avg10: f64,
    pub memory_full_avg10: f64,
    pub io_some_avg10: f64,
    pub source: String,
}

impl PsiSnapshot {
    pub fn idle() -> Self {
        Self {
            memory_some_avg10: 0.0,
            memory_full_avg10: 0.0,
            io_some_avg10: 0.0,
            source: "unavailable".into(),
        }
    }
}

pub fn read_snapshot(memory_path: &Path, io_path: &Path) -> PsiSnapshot {
    match parse_pressure_file(&fs::read_to_string(memory_path).unwrap_or_default()) {
        Some((some, full)) => {
            let io = parse_pressure_file(&fs::read_to_string(io_path).unwrap_or_default())
                .map(|(io_some, _)| io_some)
                .unwrap_or(0.0);
            PsiSnapshot {
                memory_some_avg10: some,
                memory_full_avg10: full,
                io_some_avg10: io,
                source: memory_path.display().to_string(),
            }
        }
        None => PsiSnapshot::idle(),
    }
}

pub fn parse_pressure(text: &str) -> Option<(f64, f64)> {
    parse_pressure_file(text)
}

fn parse_pressure_file(text: &str) -> Option<(f64, f64)> {
    let mut some = None;
    let mut full = None;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(kind) = parts.next() else {
            continue;
        };
        let avg10 = parts.find_map(|token| {
            token
                .strip_prefix("avg10=")
                .and_then(|value| value.parse().ok())
        });
        match kind {
            "some" => some = avg10,
            "full" => full = avg10,
            _ => {}
        }
    }
    Some((some?, full?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kernel_format() {
        let text = "\
some avg10=55.50 avg60=10.00 avg300=2.00 total=1000
full avg10=5.25 avg60=1.00 avg300=0.10 total=100
";
        let (some, full) = parse_pressure(text).unwrap();
        assert_eq!(some, 55.50);
        assert_eq!(full, 5.25);
    }
}
