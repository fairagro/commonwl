use tower_lsp_server::ls_types::{Diagnostic, DiagnosticSeverity, Position, Range};

pub fn parse_and_check(text: &str) -> Vec<Diagnostic> {
    let result = cwl_core::from_str(text);

    match result {
        Ok(_) => vec![],
        Err(e) => {
            let message = e.to_string();
            let (line, offset, len) = position_from_error(e);

            vec![Diagnostic {
                range: Range {
                    start: Position {
                        line,
                        character: offset,
                    },
                    end: Position {
                        line,
                        character: offset + len,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some(env!("CARGO_CRATE_NAME").to_owned()),
                message,
                ..Default::default()
            }]
        }
    }
}

fn position_from_error(e: cwl_core::Error) -> (u32, u32, u32) {
    match e {
        cwl_core::Error::ParsingFailed(error) => {
            let rest = match error.location() {
                Some(r) => r,
                None => return (0, 0, 0),
            };

            (
                rest.line().try_into().unwrap_or_default(),
                rest.span().offset().try_into().unwrap_or_default(),
                rest.span().len().try_into().unwrap_or_default(),
            )
        }
        _ => (0, 0, 0),
    }
}
