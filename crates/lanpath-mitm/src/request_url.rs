use http::Uri;

/// CONNECT authorities always carry a port; public script URLs omit scheme defaults.
/// Keep escaped path/query bytes and non-default authorities unchanged.
pub(crate) fn request_url(uri: &Uri) -> String {
    if !matches!(
        (uri.scheme_str(), uri.port_u16()),
        (Some("http"), Some(80)) | (Some("https"), Some(443))
    ) {
        return uri.to_string();
    }
    let (Some(scheme), Some(authority)) = (uri.scheme_str(), uri.authority()) else {
        return uri.to_string();
    };
    let Some((host, _)) = authority.as_str().rsplit_once(':') else {
        return uri.to_string();
    };
    let path = uri.path_and_query().map_or("/", |path| path.as_str());
    let mut url = String::with_capacity(scheme.len() + 3 + host.len() + path.len());
    url.push_str(scheme);
    url.push_str("://");
    url.push_str(host);
    url.push_str(path);
    url
}

#[cfg(test)]
mod tests {
    use super::request_url;

    #[test]
    fn defaults_preserve_ipv6_and_escaped_path_query() {
        for (input, expected) in [
            ("https://[::1]:443/a%2Fb?q=%2F", "https://[::1]/a%2Fb?q=%2F"),
            (
                "http://example.invalid:80/?q=%23",
                "http://example.invalid/?q=%23",
            ),
            (
                "https://example.invalid:8443/a",
                "https://example.invalid:8443/a",
            ),
            (
                "http://example.invalid:443/a",
                "http://example.invalid:443/a",
            ),
            ("/relative?q=%2F", "/relative?q=%2F"),
        ] {
            assert_eq!(request_url(&input.parse().unwrap()), expected);
        }
    }
}
