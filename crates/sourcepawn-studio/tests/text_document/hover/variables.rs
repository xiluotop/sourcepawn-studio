use insta::assert_json_snapshot;
use sourcepawn_studio::fixture::hover;

#[test]
fn global_1() {
    assert_json_snapshot!(hover(
        r#"
%! main.sp
public const int MaxClients;   /**< Maximum number of players the server supports (dynamic) */
                    |
                    ^
"#,
    ));
}

#[test]
fn int64_global() {
    assert_json_snapshot!(hover(
        r#"
%! main.sp
int64 largeValue = 2147483648;
        |
        ^
"#,
    ));
}
