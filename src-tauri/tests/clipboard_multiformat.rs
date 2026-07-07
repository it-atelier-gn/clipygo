use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext};
use clipygo_lib::history::{CapturedContent, Filter, HistoryCoordinator};
use clipygo_lib::history_commands::payload_from_full_entry;
use clipygo_lib::{hash_kind, prepare_clipboard_write};

#[test]
#[ignore]
fn multi_format_write_round_trips_on_real_clipboard() {
    let fragment = "<p>Grüße <b>world</b></p>";
    let text = "Grüße world";
    let (cf, readback) = clipygo_lib::cf_html::wrap(fragment);

    let ctx = ClipboardContext::new().unwrap();
    ctx.set(vec![
        ClipboardContent::Html(cf),
        ClipboardContent::Text(text.to_string()),
    ])
    .unwrap();

    assert_eq!(ctx.get_text().unwrap(), text);
    assert_eq!(ctx.get_html().unwrap(), readback);
    assert!(ctx.get_html().unwrap().contains(fragment));
}

#[test]
#[ignore]
fn word_style_copy_full_pipeline() {
    let word_body = "<html>\r\n<body>\r\n<!--StartFragment--><p>Grüße <b>fett</b></p><!--EndFragment-->\r\n</body>\r\n</html>";
    let header = format!(
        "Version:1.0\r\nStartHTML:{:010}\r\nEndHTML:{:010}\r\nStartFragment:{:010}\r\nEndFragment:{:010}\r\nSourceURL:file:///C:/doc.docx\r\n",
        160,
        160 + word_body.len(),
        160 + 35,
        160 + word_body.len() - 42
    );
    assert!(header.len() <= 160);
    let mut padded = header;
    while padded.len() < 160 {
        padded.push(' ');
    }
    let word_cf_html = format!("{padded}{word_body}");
    let word_rtf = r"{\rtf1\ansi Gr\'fc\'dfe \b fett\b0}";
    let word_text = "Grüße fett";

    let ctx = ClipboardContext::new().unwrap();
    ctx.set(vec![
        ClipboardContent::Html(word_cf_html),
        ClipboardContent::Rtf(word_rtf.to_string()),
        ClipboardContent::Text(word_text.to_string()),
    ])
    .unwrap();

    let read_html = ctx.get_html().unwrap();
    let read_rtf = ctx.get_rich_text().unwrap();
    let read_text = ctx.get_text().unwrap();
    assert_eq!(read_html, word_body);
    assert_eq!(read_rtf, word_rtf);
    assert_eq!(read_text, word_text);

    let mut coord = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
    let id = coord
        .insert_captured(
            CapturedContent {
                html: Some(read_html),
                rtf: Some(read_rtf),
                text: Some(read_text),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    let list = coord.list(&Filter::default(), 0, 10).unwrap();
    assert_eq!(list[0].kind_tag, "html");

    let full = coord.get_full_entry(id).unwrap().unwrap();
    let payload = payload_from_full_entry(full);
    assert_eq!(payload.text.as_deref(), Some(word_text));
    assert_eq!(payload.rtf.as_deref(), Some(word_rtf));

    let (contents, suppress) = prepare_clipboard_write(&payload).unwrap();
    ctx.set(contents).unwrap();

    let final_html = ctx.get_html().unwrap();
    let final_rtf = ctx.get_rich_text().unwrap();
    let final_text = ctx.get_text().unwrap();
    assert!(final_html.contains("<p>Grüße <b>fett</b></p>"));
    assert_eq!(final_rtf, word_rtf);
    assert_eq!(final_text, word_text);
    assert_eq!(suppress, hash_kind("html", final_html.as_bytes()));
}

#[test]
#[ignore]
fn existing_db_opens_with_new_schema() {
    let path = match std::env::var("CLIPYGO_DB") {
        Ok(p) => p,
        Err(_) => return,
    };
    let coord =
        HistoryCoordinator::new_persisted(path.into(), [0u8; 32], 64 * 1024 * 1024).unwrap();
    let list = coord.list(&Filter::default(), 0, 50).unwrap();
    let stats = coord.stats().unwrap();
    println!("entries listed: {}, items: {}", list.len(), stats.items);
    assert!(stats.items > 0);
}
