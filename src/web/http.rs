use alloc::string::String;

pub fn expects_continue(request_headers: &[u8]) -> bool {
    let Ok(header) = core::str::from_utf8(request_headers) else {
        return false;
    };
    let header = header.split("\r\n\r\n").next().unwrap_or(header);
    for line in header.lines() {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("expect")
                && value.trim().eq_ignore_ascii_case("100-continue")
            {
                return true;
            }
        }
    }
    false
}

pub fn split_target(target: &str) -> (&str, &str) {
    target.split_once('?').unwrap_or((target, ""))
}

pub fn params(query: &str, body: &[u8]) -> String {
    if !query.is_empty() {
        String::from(query)
    } else {
        core::str::from_utf8(body)
            .map(String::from)
            .unwrap_or_else(|_| String::new())
    }
}

pub fn request_body(request: &[u8]) -> Option<&[u8]> {
    body_start(request).map(|start| &request[start..])
}

pub fn body_start(request: &[u8]) -> Option<usize> {
    request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

pub fn content_length(request: &[u8]) -> Option<usize> {
    let header = core::str::from_utf8(request).ok()?;
    let header = header.split("\r\n\r\n").next().unwrap_or(header);
    for line in header.lines() {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                return value.trim().parse().ok();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::routes;

    #[test]
    fn parses_method_and_target_from_request_line() {
        let request =
            b"POST /api/identity/create?name=Trigger+Happy HTTP/1.1\r\nHost: portal\r\n\r\n";

        assert_eq!(routes::request_method(request), Some("POST"));
        assert_eq!(
            routes::request_target(request),
            Some("/api/identity/create?name=Trigger+Happy")
        );
    }

    #[test]
    fn splits_path_and_query() {
        assert_eq!(
            split_target("/api/identity/create?name=Trigger+Happy"),
            ("/api/identity/create", "name=Trigger+Happy")
        );
        assert_eq!(split_target("/status"), ("/status", ""));
    }

    #[test]
    fn extracts_content_length_case_insensitively() {
        let request = b"POST /api/identity/create HTTP/1.1\r\nhost: portal\r\ncontent-length: 35\r\n\r\nignored";

        assert_eq!(content_length(request), Some(35));
    }

    #[test]
    fn detects_expect_continue_case_insensitively() {
        let request =
            b"POST /api/entity/upload HTTP/1.1\r\nExpect: 100-continue\r\nContent-Length: 320\r\n\r\n";

        assert!(expects_continue(request));
        assert!(!expects_continue(
            b"POST /api/entity/upload HTTP/1.1\r\nContent-Length: 320\r\n\r\n"
        ));
    }

    #[test]
    fn extracts_request_body_after_header_separator() {
        let request = b"POST /api/identity/create HTTP/1.1\r\nContent-Length: 32\r\n\r\nname=Trigger+Happy&character_id=21";

        assert_eq!(
            request_body(request),
            Some(&b"name=Trigger+Happy&character_id=21"[..])
        );
    }

    #[test]
    fn prefers_query_params_over_body_params() {
        assert_eq!(params("id=7", b"id=9&name=ignored").as_str(), "id=7");
        assert_eq!(
            params("", b"name=Preston%27s+Trigger+Happy").as_str(),
            "name=Preston%27s+Trigger+Happy"
        );
    }
}
