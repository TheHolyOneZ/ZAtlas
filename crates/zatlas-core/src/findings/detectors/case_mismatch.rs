use crate::findings::{Evidence, Finding, FindingKind, FindingsInput, Severity};

pub fn detect(input: &FindingsInput) -> Vec<Finding> {
    input
        .graph
        .case_mismatches
        .iter()
        .filter_map(|m| {
            let from = input.graph.file(m.from)?;
            Some(Finding {
                kind: FindingKind::CaseMismatch,
                severity: Severity::High,
                score: 0.86,
                headline: format!(
                    "{}:{} imports \"{}\" but the file is {}",
                    from.path, m.line, m.specifier, m.actual
                ),
                files: vec![m.from],
                evidence: vec![
                    Evidence::new("Written as", &m.specifier),
                    Evidence::new("Actual file", &m.actual),
                    Evidence::new("Line", m.line),
                ],
                why: "The import differs from the filename only in case. Windows \
                      and macOS filesystems ignore case, so this resolves on the \
                      machine it was written on — and fails on Linux, which is \
                      usually where CI runs."
                    .into(),
                how_to_fix: format!(
                    "Change the import to match the file exactly: \"{}\". If the \
                     filename is the thing that is wrong, rename it with `git mv` \
                     so the change is recorded — git on a case-insensitive \
                     filesystem will not notice a plain rename.",
                    m.actual
                ),
            })
        })
        .collect()
}
