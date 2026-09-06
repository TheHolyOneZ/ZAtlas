use std::path::{Path, PathBuf};

pub fn rel_to_path(root: &Path, rel: &str) -> PathBuf {
    let mut out = root.to_path_buf();
    for segment in rel.split('/') {
        if !segment.is_empty() && segment != "." {
            out.push(segment);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_is_joined_one_component_at_a_time() {
        let root = Path::new("/repo");
        assert_eq!(
            rel_to_path(root, "src/a.ts"),
            PathBuf::from("/repo/src/a.ts")
        );
    }

    #[test]
    fn a_leading_dot_and_empty_segments_are_dropped() {
        let root = Path::new("/repo");
        assert_eq!(rel_to_path(root, "./a//b"), PathBuf::from("/repo/a/b"));
    }

    #[test]
    fn an_empty_relative_path_is_the_root_itself() {
        assert_eq!(rel_to_path(Path::new("/repo"), ""), PathBuf::from("/repo"));
    }
}
