use std::collections::HashSet;

use tauri::State;
use tauri_plugin_dialog::DialogExt;
use zatlas_core::export::{to_d2, to_markdown, to_mermaid, DiagramOptions, DiagramScope};
use zatlas_core::findings::FindingKind;
use zatlas_core::model::FileId;

use crate::error::{CommandError, Response};
use crate::state::AppState;

fn not_loaded() -> CommandError {
    CommandError::new(
        "not_loaded",
        "No analysis is loaded",
        "Open a repository and let the scan finish first.",
        false,
    )
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub format: String,

    pub scope: Option<String>,

    pub only: Option<Vec<u32>>,
}

#[tauri::command]
pub async fn export_text(state: State<'_, AppState>, request: ExportRequest) -> Response<String> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;
    let g = &loaded.analysis.graph;

    let cycle_files: HashSet<FileId> = loaded
        .findings
        .iter()
        .filter(|f| f.kind == FindingKind::Cycle)
        .flat_map(|f| f.files.iter().copied())
        .collect();

    let opts = DiagramOptions {
        scope: match request.scope.as_deref() {
            Some("files") => DiagramScope::Files,
            _ => DiagramScope::Modules,
        },
        only: request
            .only
            .unwrap_or_default()
            .into_iter()
            .map(FileId)
            .collect(),
        ..Default::default()
    };

    Ok(match request.format.as_str() {
        "d2" => to_d2(g, &opts, &cycle_files),
        "mermaid" => to_mermaid(g, &opts, &cycle_files),
        "markdown" => to_markdown(
            g,
            &loaded.deps,
            &loaded.findings,
            &loaded.history,
            &loaded.info.name,
        ),
        other => {
            return Err(CommandError::new(
                "unknown_format",
                format!("Unknown export format {other:?}"),
                "Choose D2, Mermaid or Markdown.",
                false,
            ))
        }
    })
}

#[tauri::command]
pub async fn export_to_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ExportRequest,
) -> Response<Option<String>> {
    let (extension, suggested) = match request.format.as_str() {
        "d2" => ("d2", "zatlas.d2"),
        "mermaid" => ("mmd", "zatlas.mmd"),
        _ => ("md", "zatlas-report.md"),
    };

    let text = export_text(state, request).await?;

    let path = app
        .dialog()
        .file()
        .set_file_name(suggested)
        .add_filter(extension, &[extension])
        .blocking_save_file();

    let Some(path) = path else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| {
        CommandError::new(
            "bad_path",
            e,
            "Choose a location on a local filesystem.",
            true,
        )
    })?;

    std::fs::write(&path, text).map_err(|e| {
        CommandError::new(
            "write_failed",
            format!("{}: {e}", path.display()),
            "Check you have permission to write there, and that the disk is not full.",
            true,
        )
    })?;

    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn save_image(
    app: tauri::AppHandle,
    format: String,
    suggested: String,
    data: String,
) -> Response<Option<String>> {
    let extension = match format.as_str() {
        "svg" => "svg",
        "png" => "png",
        other => {
            return Err(CommandError::new(
                "unknown_format",
                format!("Unknown image format {other:?}"),
                "Choose SVG or PNG.",
                false,
            ))
        }
    };

    let bytes = if extension == "png" {
        decode_data_url(&data).ok_or_else(|| {
            CommandError::new(
                "bad_image",
                "The rendered image could not be decoded",
                "Try again; if it keeps happening, export SVG instead.",
                true,
            )
        })?
    } else {
        data.into_bytes()
    };

    let path = app
        .dialog()
        .file()
        .set_file_name(&suggested)
        .add_filter(extension, &[extension])
        .blocking_save_file();

    let Some(path) = path else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| {
        CommandError::new(
            "bad_path",
            e,
            "Choose a location on a local filesystem.",
            true,
        )
    })?;

    std::fs::write(&path, bytes).map_err(|e| {
        CommandError::new(
            "write_failed",
            format!("{}: {e}", path.display()),
            "Check you have permission to write there, and that the disk is not full.",
            true,
        )
    })?;

    Ok(Some(path.to_string_lossy().into_owned()))
}

fn decode_data_url(input: &str) -> Option<Vec<u8>> {
    let payload = input.split_once("base64,").map(|(_, rest)| rest)?;
    decode_base64(payload)
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    fn value(byte: u8) -> Option<u32> {
        Some(match byte {
            b'A'..=b'Z' => u32::from(byte - b'A'),
            b'a'..=b'z' => u32::from(byte - b'a') + 26,
            b'0'..=b'9' => u32::from(byte - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    }

    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            break;
        }
        accumulator = (accumulator << 6) | value(byte)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes_the_three_padding_cases() {
        assert_eq!(decode_base64("YWJj").unwrap(), b"abc");
        assert_eq!(decode_base64("YWI=").unwrap(), b"ab");
        assert_eq!(decode_base64("YQ==").unwrap(), b"a");
        assert_eq!(decode_base64("").unwrap(), b"");
    }

    #[test]
    fn base64_handles_the_whole_alphabet_including_the_symbols() {
        let bytes: Vec<u8> = (0u8..=255).collect();
        let encoded = encode_for_test(&bytes);
        assert_eq!(decode_base64(&encoded).unwrap(), bytes);
    }

    #[test]
    fn base64_ignores_the_line_wrapping_browsers_add() {
        assert_eq!(decode_base64("YWJj\nZGVm\r\n").unwrap(), b"abcdef");
    }

    #[test]
    fn a_character_outside_the_alphabet_is_rejected_rather_than_skipped() {
        assert!(decode_base64("YWJ$").is_none());
    }

    #[test]
    fn a_data_url_prefix_is_stripped() {
        assert_eq!(
            decode_data_url("data:image/png;base64,YWJj").unwrap(),
            b"abc"
        );
    }

    #[test]
    fn something_that_is_not_a_data_url_is_declined() {
        assert!(decode_data_url("YWJj").is_none());
        assert!(decode_data_url("").is_none());
    }

    fn encode_for_test(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(ALPHABET[((n >> (18 - i * 6)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }
}
