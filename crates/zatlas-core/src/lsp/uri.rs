use std::path::{Path, PathBuf};

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/' | b':')
}

pub fn path_to_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");

    let text = text
        .strip_prefix("//?/UNC/")
        .map(|rest| format!("//{rest}"))
        .unwrap_or_else(|| text.strip_prefix("//?/").unwrap_or(&text).to_owned());

    let mut out = String::from("file://");

    if !text.starts_with('/') {
        out.push('/');
    }
    for b in text.bytes() {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;

    let rest = if let Some(stripped) = rest.strip_prefix('/') {
        stripped
    } else {
        return Some(PathBuf::from(
            decode(&format!("//{rest}")).replace('/', std::path::MAIN_SEPARATOR_STR),
        ));
    };

    let decoded = decode(rest);

    let is_drive = decoded
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphabetic)
        && decoded.as_bytes().get(1) == Some(&b':');
    let absolute = if is_drive {
        decoded
    } else {
        format!("/{decoded}")
    };
    Some(PathBuf::from(absolute))
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unix_path_round_trips() {
        let p = Path::new("/home/z/src/main.rs");
        assert_eq!(path_to_uri(p), "file:///home/z/src/main.rs");
        assert_eq!(uri_to_path("file:///home/z/src/main.rs").unwrap(), p);
    }

    #[test]
    fn a_windows_path_gains_the_third_slash_and_keeps_its_colon() {
        let uri = path_to_uri(Path::new(r"C:\Users\z\main.rs"));
        assert_eq!(uri, "file:///C:/Users/z/main.rs");
        assert_eq!(
            uri_to_path(&uri).unwrap(),
            PathBuf::from("C:/Users/z/main.rs")
        );
    }

    #[test]
    fn spaces_and_non_ascii_are_percent_encoded() {
        let uri = path_to_uri(Path::new("/home/z/my code/café.ts"));
        assert_eq!(uri, "file:///home/z/my%20code/caf%C3%A9.ts");
        assert_eq!(
            uri_to_path(&uri).unwrap(),
            PathBuf::from("/home/z/my code/café.ts")
        );
    }

    #[test]
    fn a_verbatim_prefix_is_stripped_because_no_server_understands_it() {
        assert_eq!(
            path_to_uri(Path::new(r"\\?\C:\src\a.rs")),
            "file:///C:/src/a.rs"
        );
    }

    #[test]
    fn a_non_file_scheme_is_declined_rather_than_guessed_at() {
        assert!(uri_to_path("untitled:Untitled-1").is_none());
        assert!(uri_to_path("jdt://contents/rt.jar").is_none());
    }

    #[test]
    fn a_stray_percent_is_left_alone_instead_of_eating_the_next_characters() {
        assert_eq!(decode("100% done"), "100% done");
        assert_eq!(decode("a%zz"), "a%zz");
    }
}
