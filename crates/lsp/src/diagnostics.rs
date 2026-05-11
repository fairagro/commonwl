use cwl_core::documents::CWLDocument;
use tower_lsp_server::ls_types::{Diagnostic, DiagnosticSeverity, Position, Range};

pub fn parse_and_check(text: &str) -> (Option<CWLDocument>, Vec<Diagnostic>) {
    let result = cwl_core::from_str(text);

    match result {
        Ok(doc) => {
            eprintln!("[diag] parsed OK");
            (Some(doc), vec![])
        }
        Err(e) => match e {
            cwl_core::Error::ParsingFailed(e) => {
                let message = e.to_string();
                eprintln!("[diag] parsed error {message}");
                if let Some((start, end)) =
                    position_from_error(text, cwl_core::Error::ParsingFailed(e))
                {
                    (
                        None,
                        vec![Diagnostic {
                            range: Range { start, end },
                            severity: Some(DiagnosticSeverity::ERROR),
                            source: Some(env!("CARGO_CRATE_NAME").to_owned()),
                            message,
                            ..Default::default()
                        }],
                    )
                } else {
                    (None, vec![])
                }
            }
            _ => (None, vec![]),
        },
    }
}

fn position_from_error(text: &str, e: cwl_core::Error) -> Option<(Position, Position)> {
    match e {
        cwl_core::Error::ParsingFailed(error) => {
            let rest = error.location()?;

            let start_offset = rest.span().offset();
            let end_offset = start_offset + rest.span().len();

            Some((
                offset_to_position(text, start_offset),
                offset_to_position(text, end_offset),
            ))
        }
        _ => None,
    }
}

pub fn offset_to_position(text: &str, offset: u64) -> Position {
    let mut line = 0;
    let mut col = 0;

    for (i, ch) in text.char_indices() {
        if i as u64 >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    Position::new(line, col)
}
