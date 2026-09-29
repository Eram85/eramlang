//! Professional compiler diagnostics: errors, warnings, notes with
//! filename/line/column and a caret pointing at the offending source.

use colored::*;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "{}", "error".red().bold()),
            Severity::Warning => write!(f, "{}", "warning".yellow().bold()),
            Severity::Note => write!(f, "{}", "note".cyan().bold()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Option<String>,
    pub message: String,
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(file: &str, line: usize, col: usize, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            code: None,
            message: message.into(),
            file: file.to_string(),
            line,
            col,
            help: None,
        }
    }

    pub fn with_code(mut self, code: &str) -> Self {
        self.code = Some(code.to_string());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Pretty-print this diagnostic against the given source text.
    pub fn render(&self, source: &str) -> String {
        let mut out = String::new();
        let code_str = self
            .code
            .as_ref()
            .map(|c| format!("[{}]", c))
            .unwrap_or_default();
        out.push_str(&format!(
            "{}{}: {}\n",
            self.severity,
            code_str,
            self.message.bold()
        ));
        out.push_str(&format!(
            " {} {}:{}:{}\n",
            "-->".blue().bold(),
            self.file,
            self.line,
            self.col
        ));

        if let Some(src_line) = source.lines().nth(self.line.saturating_sub(1)) {
            let line_num_str = self.line.to_string();
            let gutter = " ".repeat(line_num_str.len());
            out.push_str(&format!("{} {}\n", gutter, "|".blue().bold()));
            out.push_str(&format!(
                "{} {} {}\n",
                line_num_str.blue().bold(),
                "|".blue().bold(),
                src_line
            ));
            let caret_pad = " ".repeat(self.col.saturating_sub(1));
            out.push_str(&format!(
                "{} {} {}{}\n",
                gutter,
                "|".blue().bold(),
                caret_pad,
                "^".red().bold()
            ));
        }

        if let Some(help) = &self.help {
            out.push_str(&format!("  = {}: {}\n", "help".green().bold(), help));
        }

        out
    }
}

/// A collection of diagnostics accumulated during a compilation phase.
#[derive(Debug, Default)]
pub struct DiagnosticBag {
    pub diagnostics: Vec<Diagnostic>,
}

impl DiagnosticBag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn render_all(&self, source: &str) -> String {
        self.diagnostics
            .iter()
            .map(|d| d.render(source))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
