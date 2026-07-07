const HEADER: &str = "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n";
const WRAP_PREFIX: &str = "<html>\r\n<body>\r\n<!--StartFragment-->";
const WRAP_SUFFIX: &str = "<!--EndFragment-->\r\n</body>\r\n</html>";

fn is_full_document(fragment: &str) -> bool {
    let trimmed = fragment.trim_start();
    let head: String = trimmed
        .chars()
        .take(9)
        .collect::<String>()
        .to_ascii_lowercase();
    head.starts_with("<html") || head.starts_with("<!doctype")
}

pub fn wrap(fragment: &str) -> (String, String) {
    let (prefix, suffix) = if is_full_document(fragment) {
        ("", "")
    } else {
        (WRAP_PREFIX, WRAP_SUFFIX)
    };
    let readback = format!("{prefix}{fragment}{suffix}");
    let start_html = HEADER.len();
    let start_fragment = start_html + prefix.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + suffix.len();
    let cf_html = format!(
        "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n{readback}"
    );
    (cf_html, readback)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_value(cf: &str, key: &str) -> usize {
        cf.lines()
            .find_map(|l| l.strip_prefix(&format!("{key}:")))
            .unwrap()
            .trim_start_matches('0')
            .parse()
            .unwrap_or(0)
    }

    #[test]
    fn offsets_slice_back_to_readback() {
        let (cf, readback) = wrap("<p>Hello <b>world</b></p>");
        let start = header_value(&cf, "StartHTML");
        let end = header_value(&cf, "EndHTML");
        assert_eq!(&cf[start..end], readback);
        assert!(readback.starts_with(WRAP_PREFIX));
        assert!(readback.ends_with(WRAP_SUFFIX));
    }

    #[test]
    fn fragment_offsets_point_at_fragment() {
        let fragment = "<p>Grüße &amp; Umläute — äöüß</p>";
        let (cf, _) = wrap(fragment);
        let start = header_value(&cf, "StartFragment");
        let end = header_value(&cf, "EndFragment");
        assert_eq!(&cf[start..end], fragment);
    }

    #[test]
    fn full_document_is_not_double_wrapped() {
        let doc = "<html>\r\n<body>\r\n<!--StartFragment--><b>x</b><!--EndFragment-->\r\n</body>\r\n</html>";
        let (cf, readback) = wrap(doc);
        assert_eq!(readback, doc);
        let start = header_value(&cf, "StartHTML");
        let end = header_value(&cf, "EndHTML");
        assert_eq!(&cf[start..end], doc);
    }

    #[test]
    fn doctype_and_case_insensitive_html_detected() {
        assert!(is_full_document(
            "<!DOCTYPE html><html><body>x</body></html>"
        ));
        assert!(is_full_document("<HTML><body>x</body></HTML>"));
        assert!(is_full_document(
            "  <html lang=\"de\"><body>x</body></html>"
        ));
        assert!(!is_full_document("<p>x</p>"));
    }

    #[test]
    fn header_length_matches_template() {
        let (cf, _) = wrap("<p>x</p>");
        assert_eq!(header_value(&cf, "StartHTML"), HEADER.len());
        assert_eq!(cf[..HEADER.len()].len(), HEADER.len());
        assert!(cf[HEADER.len()..].starts_with(WRAP_PREFIX));
    }
}
