use sourcepawn_studio::fixture::TestBed;
use std::{thread, time::Duration};

/// Regression test for the "no entry found for key" panic reported against
/// `GlobalStateSnapshot::file_line_index`.
///
/// A file that is discovered by the workspace file loader (i.e. not opened
/// through `textDocument/didOpen`) is registered in the analysis' source
/// roots so `#include` resolution can find it and goto-definition can target
/// it. If that file's bytes are not valid UTF-8, its content used to be
/// dropped entirely, which meant it never got an entry inserted in the line
/// endings map. Any subsequent goto-definition targeting that file then
/// panicked when computing the line index for the response.
#[test]
fn goto_definition_on_include_with_invalid_utf8_content() {
    let fixture = r#"
%! main.sp
#include "foo.inc"
           |
           ^
"#;

    let test_bed = TestBed::new(fixture, false).unwrap();

    // `foo.inc` is intentionally *not* part of the fixture text (Rust string
    // literals can't contain invalid UTF-8), so it is written directly to
    // disk with a byte sequence that is not valid UTF-8. It will only be
    // picked up by the background workspace file loader, not by
    // `textDocument/didOpen`.
    let foo_inc_path = test_bed.directory().join("foo.inc");
    std::fs::write(
        &foo_inc_path,
        [b'i', b'n', b't', b' ', b'f', b'o', b'o', b';', 0xFF],
    )
    .unwrap();

    test_bed
        .initialize(
            serde_json::from_value(serde_json::json!({
                "textDocument": {
                    "definition": {
                        "linkSupport": true
                    }
                },
                "workspace": {
                    "configuration": true,
                    "workspace_folders": true
                }
            }))
            .unwrap(),
        )
        .unwrap();

    // Give the background file loader time to discover and load `foo.inc`.
    thread::sleep(Duration::from_millis(500));

    let text_document_position = test_bed.cursor().unwrap();
    let params = lsp_types::request::GotoTypeDefinitionParams {
        text_document_position_params: text_document_position,
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };

    let response = test_bed
        .client()
        .send_request::<lsp_types::request::GotoDefinition>(params)
        .expect("goto-definition should not panic on a non-UTF-8 include file");

    assert!(
        response.is_some(),
        "expected a location to be resolved for `foo`, got: {response:?}"
    );
}

/// Real-world variant of the above, using the actual `tf2items.inc` shipped
/// in the official `tf2items-1.6.4-hg279` releases (both the Windows and
/// Linux zips, dated 2015-12-17, MD5 `4d4ec8cfa71df4af94c0de470cacff51`).
/// Its ASCII-art footer comment uses Windows-1252 bytes for a handful of
/// special characters, making the whole file invalid UTF-8. This is the
/// exact file and `#include <...>` (chevron) syntax originally reported to
/// crash the extension.
#[test]
#[allow(invalid_from_utf8)]
fn goto_definition_on_official_tf2items_inc_chevron_include() {
    let real_bytes = include_bytes!("fixtures/tf2items_invalid_utf8.inc");
    assert!(
        std::str::from_utf8(real_bytes).is_err(),
        "sanity check: the fixture file is expected to be invalid UTF-8"
    );

    let fixture = r#"
%! main.sp
#include <tf2items>
           |
           ^
"#;

    let test_bed = TestBed::new(fixture, false).unwrap();

    let include_dir = test_bed.directory().join("include");
    std::fs::create_dir_all(&include_dir).unwrap();
    std::fs::write(include_dir.join("tf2items.inc"), real_bytes).unwrap();

    test_bed
        .initialize(
            serde_json::from_value(serde_json::json!({
                "textDocument": {
                    "definition": {
                        "linkSupport": true
                    }
                },
                "workspace": {
                    "configuration": true,
                    "workspace_folders": true
                }
            }))
            .unwrap(),
        )
        .unwrap();

    // Give the background file loader time to discover and load `tf2items.inc`.
    thread::sleep(Duration::from_millis(500));

    let text_document_position = test_bed.cursor().unwrap();
    let params = lsp_types::request::GotoTypeDefinitionParams {
        text_document_position_params: text_document_position,
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };

    let response = test_bed
        .client()
        .send_request::<lsp_types::request::GotoDefinition>(params)
        .expect("goto-definition should not panic on the official tf2items.inc");

    assert!(
        response.is_some(),
        "expected a location to be resolved for `tf2items`, got: {response:?}"
    );
}
