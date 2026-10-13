use super::*;

#[test]
fn reads_plain_and_chunked_answers() {
    let plain = parse(b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\n\r\n").expect("plain");
    assert_eq!(
        plain,
        Answer {
            status: 204,
            body: String::new()
        }
    );
    let chunked = parse(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n6\r\n:\"è\"}\r\n0\r\n\r\n"
            .as_bytes(),
    )
    .expect("chunked");
    assert_eq!(chunked.body, "{\"a\":\"è\"}");
    assert!(parse(b"not http").is_err());
    assert!(parse(b"HTTP/1.1 nope\r\n\r\n").is_err());
}

#[tokio::test]
async fn says_why_there_is_no_answer() {
    let missing = exchange(
        Path::new("/nonexistent/socket"),
        "GET",
        "/",
        &[],
        "",
        Duration::from_secs(1),
    )
    .await;
    assert!(missing.expect_err("no socket").starts_with("not reachable"));
}
