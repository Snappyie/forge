//! Output formatting and exit codes (spec 16.8).
//!
//! Two contracts here are load-bearing:
//!
//! * `--output json` MUST be machine-readable and stable, so a script can
//!   parse it without scraping prose.
//! * Errors go to stderr, so stdout carries only data.

use serde_json::Value;

/// Exit codes from spec 16.8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    Success = 0,
    GeneralFailure = 1,
    UsageError = 2,
    AuthenticationFailure = 3,
    AuthorizationFailure = 4,
    NotFound = 5,
    Conflict = 6,
    ValidationFailure = 7,
    NetworkFailure = 8,
}

impl ExitCode {
    pub fn as_i32(&self) -> i32 {
        *self as u8 as i32
    }
}

/// How results are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum OutputFormat {
    /// Aligned columns, for a human.
    #[default]
    Table,
    /// Stable JSON, for a script.
    Json,
    /// YAML, for a human or a script that prefers it.
    Yaml,
}

/// Anything that can be shown to the user.
pub trait Render {
    /// A one-line summary shown above any detail.
    fn summary(&self) -> String;

    /// Rows for table output: a header plus a row per record.
    fn table(&self) -> Table;

    /// The machine-readable form.
    fn data(&self) -> Value;
}

/// A simple aligned table.
#[derive(Debug, Clone, Default)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// Accepts anything string-like, so a caller can pass either a literal
    /// slice of `&str` or a `Vec<String>` of computed column names.
    pub fn new<I, S>(headers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            headers: headers.into_iter().map(|h| h.as_ref().to_string()).collect(),
            rows: Vec::new(),
        }
    }

    pub fn push(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    /// Renders with columns padded to their widest cell.
    ///
    /// Width is computed per column so a long value in one row does not stretch
    /// every other column.
    pub fn render(&self) -> String {
        if self.headers.is_empty() {
            return String::new();
        }

        let mut widths: Vec<usize> = self
            .headers
            .iter()
            .map(|h| h.chars().count())
            .collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(cell.chars().count());
                }
            }
        }

        let mut out = String::new();
        let render_row = |cells: &[String], widths: &[usize]| {
            let line = cells
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    if i + 1 == cells.len() {
                        // The final column needs no trailing padding.
                        c.clone()
                    } else {
                        format!("{c:<width$}", width = widths.get(i).copied().unwrap_or(0))
                    }
                })
                .collect::<Vec<_>>()
                .join("  ");
            line.trim_end().to_string()
        };

        out.push_str(&render_row(&self.headers, &widths));
        out.push('\n');
        out.push_str(
            &widths
                .iter()
                .map(|w| "-".repeat(*w))
                .collect::<Vec<_>>()
                .join("  "),
        );
        for row in &self.rows {
            out.push('\n');
            out.push_str(&render_row(row, &widths));
        }
        out
    }
}

/// Anything a command produced.
///
/// A command may yield either a list or a single resource, so the branches are
/// unified here rather than forcing every command into one shape.
pub enum Renderable {
    List(crate::ListView),
    Detail(crate::DetailView),
    Custom(Box<dyn Render>),
}

impl Render for Renderable {
    fn summary(&self) -> String {
        match self {
            Renderable::List(view) => view.summary(),
            Renderable::Detail(view) => view.summary(),
            Renderable::Custom(view) => view.summary(),
        }
    }

    fn table(&self) -> Table {
        match self {
            Renderable::List(view) => view.table(),
            Renderable::Detail(view) => view.table(),
            Renderable::Custom(view) => view.table(),
        }
    }

    fn data(&self) -> Value {
        match self {
            Renderable::List(view) => view.data(),
            Renderable::Detail(view) => view.data(),
            Renderable::Custom(view) => view.data(),
        }
    }
}

/// Writes a result in the requested format.
///
/// The machine-readable form comes from [`Render::data`] rather than a
/// `Serialize` bound on the view, so a view's wire shape is defined in exactly
/// one place.
pub fn emit<T: Render>(value: &T, format: OutputFormat) -> String {
    let machine = value.data();

    match format {
        OutputFormat::Json => {
            // Pretty-printed and stable: no map ordering surprises, because
            // serde_json preserves insertion order for a struct.
            serde_json::to_string_pretty(&machine).unwrap_or_default()
        }
        OutputFormat::Yaml => serde_yaml::to_string(&machine)
            .unwrap_or_else(|e| format!("could not render YAML: {e}")),
        OutputFormat::Table => {
            let table = value.table();
            let rendered = table.render();
            if rendered.trim().is_empty() {
                // An empty result still needs a machine-readable form.
                serde_json::to_string_pretty(&machine).unwrap_or_default()
            } else {
                rendered
            }
        }
    }
}


/// Prints an error to stderr, never stdout.
pub fn print_error(message: &str) {
    eprintln!("error: {message}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[test]
    fn exit_codes_match_the_specification() {
        assert_eq!(ExitCode::Success.as_i32(), 0);
        assert_eq!(ExitCode::GeneralFailure.as_i32(), 1);
        assert_eq!(ExitCode::UsageError.as_i32(), 2);
        assert_eq!(ExitCode::AuthenticationFailure.as_i32(), 3);
        assert_eq!(ExitCode::AuthorizationFailure.as_i32(), 4);
        assert_eq!(ExitCode::NotFound.as_i32(), 5);
        assert_eq!(ExitCode::Conflict.as_i32(), 6);
        assert_eq!(ExitCode::ValidationFailure.as_i32(), 7);
        assert_eq!(ExitCode::NetworkFailure.as_i32(), 8);
    }

    #[test]
    fn a_table_pads_columns_to_their_widest_cell() {
        let mut table = Table::new(["ID", "NAME"]);
        table.push(vec!["1".into(), "short".into()]);
        table.push(vec!["22".into(), "a much longer name".into()]);

        let rendered = table.render();
        let lines: Vec<&str> = rendered.lines().collect();

        assert!(lines[0].starts_with("ID"));
        assert!(lines[1].starts_with("--"));
        // The second column starts at the same offset on every row.
        let name_col = lines[0].find("NAME").unwrap();
        assert_eq!(lines[2].find("short"), Some(name_col));
        assert_eq!(lines[3].find("a much longer name"), Some(name_col));
    }

    #[test]
    fn an_empty_table_renders_only_its_header() {
        let table = Table::new(["ID", "NAME"]);
        let rendered = table.render();
        assert_eq!(rendered.lines().count(), 2, "header and rule only");
    }

    #[test]
    fn json_output_is_stable_across_renders() {
        #[derive(Serialize)]
        struct Row {
            id: String,
            name: String,
        }
        #[derive(Serialize)]
        struct Envelope {
            data: Vec<Row>,
        }
        impl Render for Envelope {
            fn summary(&self) -> String {
                format!("{} rows", self.data.len())
            }
            fn table(&self) -> Table {
                let mut t = Table::new(["ID", "NAME"]);
                for row in &self.data {
                    t.push(vec![row.id.clone(), row.name.clone()]);
                }
                t
            }
            fn data(&self) -> Value {
                serde_json::to_value(self).unwrap()
            }
        }

        let value = Envelope {
            data: vec![Row {
                id: "1".into(),
                name: "one".into(),
            }],
        };

        let first = emit(&value, OutputFormat::Json);
        let second = emit(&value, OutputFormat::Json);
        assert_eq!(first, second, "json output must be stable");
        assert!(first.contains("\"name\""));
    }

    #[test]
    fn yaml_output_is_produced() {
        #[derive(Serialize)]
        struct Row {
            id: u32,
        }
        #[derive(Serialize)]
        struct Envelope {
            data: Vec<Row>,
        }
        impl Render for Envelope {
            fn summary(&self) -> String {
                String::new()
            }
            fn table(&self) -> Table {
                Table::default()
            }
            fn data(&self) -> Value {
                serde_json::to_value(self).unwrap()
            }
        }

        let yaml = emit(&Envelope { data: vec![Row { id: 7 }] }, OutputFormat::Yaml);
        assert!(yaml.contains("id: 7"), "{yaml}");
    }

    #[test]
    fn an_empty_result_falls_back_to_machine_readable_output() {
        #[derive(Serialize)]
        struct Empty {
            data: Vec<String>,
        }
        impl Render for Empty {
            fn summary(&self) -> String {
                String::new()
            }
            fn table(&self) -> Table {
                Table::default()
            }
            fn data(&self) -> Value {
                serde_json::to_value(self).unwrap()
            }
        }

        // Table rendering of nothing must still be parseable, or a script
        // reading an empty list would get nothing at all.
        let rendered = emit(&Empty { data: vec![] }, OutputFormat::Table);
        assert!(rendered.contains("data"), "{rendered}");
    }
}