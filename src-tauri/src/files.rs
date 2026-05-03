pub fn sanitize_title(s: &str) -> String {
    let cleaned: String = s.chars().filter(|&c| c >= ' ' || c == '\t').collect();
    let trimmed = cleaned.trim();
    if trimmed.len() <= 200 {
        trimmed.to_string()
    } else {
        trimmed.chars().take(200).collect()
    }
}

pub fn sanitize_filename(s: &str) -> String {
    const INVALID: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\0'];
    let cleaned: String = s
        .chars()
        .map(|c| {
            if INVALID.contains(&c) || (c as u32) < 32 {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    let result = if trimmed.is_empty() {
        "attachment"
    } else {
        trimmed
    };
    if result.len() <= 200 {
        result.to_string()
    } else {
        result.chars().take(200).collect()
    }
}

pub fn do_copy_to_pragmadocs(src: &str, working_dir: &str) -> Result<String, String> {
    let src_path = std::path::Path::new(src);
    let canonical = src_path
        .canonicalize()
        .map_err(|_| "file not found or not accessible")?;
    let raw_name = canonical
        .file_name()
        .ok_or("invalid file name")?
        .to_string_lossy()
        .into_owned();
    let filename = sanitize_filename(&raw_name);
    let dest_dir = std::path::Path::new(working_dir).join(".pragmadocs");
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(&filename);
    std::fs::copy(&canonical, &dest).map_err(|e| e.to_string())?;
    Ok(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_invalid_chars() {
        assert_eq!(sanitize_filename("foo<>bar.txt"), "foo__bar.txt");
        assert_eq!(sanitize_filename("path/to\\file.rs"), "path_to_file.rs");
        assert_eq!(sanitize_filename("a:b?c*d.md"), "a_b_c_d.md");
    }

    #[test]
    fn sanitize_empty_becomes_attachment() {
        assert_eq!(sanitize_filename(""), "attachment");
        assert_eq!(sanitize_filename("___"), "attachment");
    }

    #[test]
    fn sanitize_truncates_at_200_chars() {
        let long = "a".repeat(300);
        let result = sanitize_filename(&long);
        assert_eq!(result.len(), 200);
    }

    #[test]
    fn sanitize_preserves_normal_names() {
        assert_eq!(sanitize_filename("report.pdf"), "report.pdf");
        assert_eq!(sanitize_filename("my file.txt"), "my file.txt");
    }

    #[test]
    fn copy_to_pragmadocs_copies_file() {
        let tmp = std::env::temp_dir().join(format!(
            "pragma_copy_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
        ));
        let src_dir = tmp.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let src_file = src_dir.join("hello.txt");
        std::fs::write(&src_file, b"content").unwrap();

        let dst_dir = tmp.join("dst");
        std::fs::create_dir_all(&dst_dir).unwrap();

        let result = do_copy_to_pragmadocs(src_file.to_str().unwrap(), dst_dir.to_str().unwrap());
        assert!(result.is_ok(), "copy failed: {:?}", result);
        assert_eq!(result.unwrap(), "hello.txt");

        let dest = dst_dir.join(".pragmadocs").join("hello.txt");
        assert!(dest.exists(), "destination file does not exist");
        assert_eq!(std::fs::read(&dest).unwrap(), b"content");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn copy_to_pragmadocs_nonexistent_src_returns_err() {
        let result = do_copy_to_pragmadocs("/nonexistent/path/file.txt", "/tmp");
        assert!(result.is_err());
    }
}
