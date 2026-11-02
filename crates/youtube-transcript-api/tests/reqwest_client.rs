//! The default `reqwest` client against a local mock server.

use wiremock::matchers::{body_string, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};
use youtube_transcript_api::http::{
    HeaderMap, HeaderValue, HttpClient, HttpRequest, Method, ReqwestClient, StatusCode,
    TransportErrorKind, header,
};
use youtube_transcript_api::proxies::{GenericProxyConfig, ProxyConfig, WebshareProxyConfig};

/// A `GET` request to `url` with `headers`.
const fn get(url: String, headers: HeaderMap) -> HttpRequest {
    HttpRequest::new(Method::GET, url, headers, None)
}

/// A `GET` sends its headers and returns the status, headers, and body.
#[tokio::test]
async fn get_sends_headers_and_returns_status_headers_and_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/watch"))
        .and(query_param("v", "abc"))
        .and(header("accept-language", "en-US"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "7")
                .set_body_string("slow down"),
        )
        .mount(&server)
        .await;

    let client = ReqwestClient::new().unwrap();
    let response = client
        .send(get(
            format!("{}/watch?v=abc", server.uri()),
            HeaderMap::from_iter([(header::ACCEPT_LANGUAGE, HeaderValue::from_static("en-US"))]),
        ))
        .await
        .unwrap();

    assert_eq!(response.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers[header::RETRY_AFTER], "7");
    assert_eq!(response.body, "slow down");
    assert_eq!(response.url, format!("{}/watch?v=abc", server.uri()));
}

/// A `POST` sends its JSON body with the JSON media type.
#[tokio::test]
async fn post_sends_a_json_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/youtubei/v1/player"))
        .and(header("content-type", "application/json"))
        .and(body_string(r#"{"videoId":"abc"}"#))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;

    let client = ReqwestClient::new().unwrap();
    let response = client
        .send(HttpRequest::new(
            Method::POST,
            format!("{}/youtubei/v1/player", server.uri()),
            HeaderMap::new(),
            Some(r#"{"videoId":"abc"}"#.to_owned()),
        ))
        .await
        .unwrap();

    assert_eq!(response.status, StatusCode::OK);
}

/// A refused connection is a `Connect` error.
#[tokio::test]
async fn connection_failure_is_a_transport_error() {
    // A port nothing listens on: bind one, then close it.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/watch", listener.local_addr().unwrap());
    drop(listener);

    let client = ReqwestClient::new().unwrap();
    let error = client.send(get(url, HeaderMap::new())).await.unwrap_err();
    assert_eq!(error.kind, TransportErrorKind::Connect);
}

/// A server that hangs up before responding gives a `Request` error.
#[tokio::test]
async fn hang_up_before_responding_is_a_request_error() {
    // Reads the request, then closes without a response.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/watch", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _ = std::io::Read::read(&mut stream, &mut request).unwrap();
    });

    let client = ReqwestClient::new().unwrap();
    let error = client.send(get(url, HeaderMap::new())).await.unwrap_err();
    assert_eq!(error.kind, TransportErrorKind::Request);
    server.join().unwrap();
}

/// The consent cookie goes to every `youtube.com` host and nowhere else.
#[test]
fn consent_cookie_is_sent_to_every_youtube_subdomain() {
    let client = ReqwestClient::new().unwrap();
    client.set_cookie("CONSENT", "YES+cb.20210328-17-p0.de+FX+119", ".youtube.com");

    let expected = Some("CONSENT=YES+cb.20210328-17-p0.de+FX+119".to_owned());
    assert_eq!(
        client.cookies_for("https://www.youtube.com/watch?v=abc"),
        expected
    );
    assert_eq!(client.cookies_for("https://youtube.com/"), expected);
    assert_eq!(client.cookies_for("https://example.com/"), None);
}

/// Requests go through a generic proxy.
#[tokio::test]
async fn requests_go_through_the_proxy() {
    let proxy = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/watch"))
        .respond_with(ResponseTemplate::new(200).set_body_string("via proxy"))
        .mount(&proxy)
        .await;
    let config = GenericProxyConfig::new(Some(proxy.uri()), None).unwrap();
    assert_eq!(config.proxy_urls().https, proxy.uri());

    let client = ReqwestClient::with_proxy(Some(&config)).unwrap();
    let response = client
        .send(get(
            "http://www.youtube.com/watch".to_owned(),
            HeaderMap::new(),
        ))
        .await
        .unwrap();

    assert_eq!(response.body, "via proxy");
}

/// A `socks5h` proxy resolves the host itself.
#[tokio::test]
async fn socks5h_proxy_resolves_hosts_through_the_proxy() {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    let target = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/watch"))
        .respond_with(ResponseTemplate::new(200).set_body_string("via socks"))
        .mount(&target)
        .await;
    let target_addr = *target.address();

    // A one-connection SOCKS5 server (RFC 1928, no auth) that connects
    // every request to the mock server and reports the host each request
    // named.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_url = format!("socks5h://{}", listener.local_addr().unwrap());
    let socks = std::thread::spawn(move || {
        let (mut client, _) = listener.accept().unwrap();
        let mut greeting = [0_u8; 2];
        client.read_exact(&mut greeting).unwrap();
        let mut methods = vec![0_u8; usize::from(greeting[1])];
        client.read_exact(&mut methods).unwrap();
        client.write_all(&[5, 0]).unwrap();

        let mut request = [0_u8; 5];
        client.read_exact(&mut request).unwrap();
        assert_eq!(request[..4], [5, 1, 0, 3], "CONNECT to a domain name");
        let mut host = vec![0_u8; usize::from(request[4])];
        client.read_exact(&mut host).unwrap();
        let mut port = [0_u8; 2];
        client.read_exact(&mut port).unwrap();
        client.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).unwrap();

        let upstream = TcpStream::connect(target_addr).unwrap();
        let (mut from_client, mut to_upstream) =
            (client.try_clone().unwrap(), upstream.try_clone().unwrap());
        let relay = std::thread::spawn(move || std::io::copy(&mut from_client, &mut to_upstream));
        let (mut from_upstream, mut to_client) = (upstream, client);
        let _ = std::io::copy(&mut from_upstream, &mut to_client);
        let _ = relay.join();
        (String::from_utf8(host).unwrap(), u16::from_be_bytes(port))
    });

    let config = GenericProxyConfig::new(Some(proxy_url.clone()), Some(proxy_url)).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONNECTION, HeaderValue::from_static("close"));
    let response = ReqwestClient::with_proxy(Some(&config))
        .unwrap()
        .send(get("http://www.youtube.com/watch".to_owned(), headers))
        .await
        .unwrap();

    assert_eq!(response.body, "via socks");
    assert_eq!(socks.join().unwrap(), ("www.youtube.com".to_owned(), 80));
}

/// An unusable HTTP proxy URL fails the build.
#[test]
fn unusable_proxy_url_is_rejected() {
    let config = GenericProxyConfig::new(Some("::not a url::".to_owned()), None).unwrap();
    assert!(ReqwestClient::with_proxy(Some(&config)).is_err());
}

/// An unusable HTTPS proxy URL fails the build.
#[test]
fn unusable_https_proxy_url_is_rejected() {
    let config = GenericProxyConfig::new(
        Some("http://localhost:8080".to_owned()),
        Some("::not a url::".to_owned()),
    )
    .unwrap();
    assert!(ReqwestClient::with_proxy(Some(&config)).is_err());
}

/// A Webshare proxy builds a client without kept-alive connections.
#[test]
fn webshare_proxy_builds_without_keep_alive() {
    // Its `Connection: close` setting also empties the idle pool.
    let config = WebshareProxyConfig::new("user", "pass");
    assert!(config.prevent_keeping_connections_alive());
    assert!(ReqwestClient::with_proxy(Some(&config)).is_ok());
}

/// An invalid cookie domain or URL gives no cookies.
#[test]
fn cookies_need_a_valid_url_and_domain() {
    let client = ReqwestClient::new().unwrap();
    client.set_cookie("CONSENT", "YES", "not a domain");
    assert_eq!(client.cookies_for("not a url"), None);
    assert_eq!(client.cookies_for("https://www.youtube.com/"), None);
}

/// A body cut short is a `Body` error.
#[tokio::test]
async fn body_cut_short_is_a_transport_error() {
    use std::io::Write;
    // Promises ten bytes, sends two, hangs up.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/watch", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _ = std::io::Read::read(&mut stream, &mut request).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nab")
            .unwrap();
    });

    let client = ReqwestClient::new().unwrap();
    let error = client.send(get(url, HeaderMap::new())).await.unwrap_err();
    assert_eq!(error.kind, TransportErrorKind::Body);
    server.join().unwrap();
}

/// A response slower than the timeout fails as a `Request` error; without one it arrives.
#[tokio::test]
async fn a_slow_response_times_out() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_millis(600))
                .set_body_string("late"),
        )
        .mount(&server)
        .await;
    let url = format!("{}/watch?v=abc", server.uri());

    let started = std::time::Instant::now();
    let impatient =
        ReqwestClient::with_options(None, Some(std::time::Duration::from_millis(100))).unwrap();
    let error = impatient
        .send(get(url.clone(), HeaderMap::new()))
        .await
        .unwrap_err();
    assert_eq!(error.kind, TransportErrorKind::Request);
    assert!(started.elapsed() < std::time::Duration::from_millis(500));

    let patient = ReqwestClient::with_options(None, None).unwrap();
    let response = patient.send(get(url, HeaderMap::new())).await.unwrap();
    assert_eq!(response.body, "late");
}

/// A response whose `Content-Length` is over the limit fails before the
/// client reads the body, and one at the limit arrives.
#[tokio::test]
async fn a_body_over_the_limit_by_content_length_is_refused() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("0123456789"))
        .mount(&server)
        .await;
    let url = format!("{}/watch", server.uri());

    let client = ReqwestClient::new().unwrap().with_max_body_bytes(9);
    let error = client
        .send(get(url.clone(), HeaderMap::new()))
        .await
        .unwrap_err();
    assert_eq!(error.kind, TransportErrorKind::BodyTooLarge);
    assert_eq!(
        error.message,
        "the response body is over the limit of 9 bytes"
    );

    let exact = ReqwestClient::new().unwrap().with_max_body_bytes(10);
    let response = exact.send(get(url, HeaderMap::new())).await.unwrap();
    assert_eq!(response.body, "0123456789");
}

/// Serves one chunked response of `chunks` (no `Content-Length`) and gives
/// the URL.
fn serve_chunked(chunks: &'static [&'static str]) -> (String, std::thread::JoinHandle<()>) {
    use std::io::Write;
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| unreachable!("{e}"));
    let address = listener
        .local_addr()
        .unwrap_or_else(|e| unreachable!("{e}"));
    let url = format!("http://{address}/watch");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap_or_else(|e| unreachable!("{e}"));
        let mut request = [0_u8; 1024];
        let _ = std::io::Read::read(&mut stream, &mut request);
        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n");
        for chunk in chunks {
            let _ = stream.write_all(format!("{:x}\r\n{chunk}\r\n", chunk.len()).as_bytes());
        }
        let _ = stream.write_all(b"0\r\n\r\n");
    });
    (url, server)
}

/// Without a `Content-Length`, the limit applies as chunks arrive.
#[tokio::test]
async fn a_chunked_body_over_the_limit_is_refused() {
    let (url, server) = serve_chunked(&["hello", "world"]);
    let client = ReqwestClient::new().unwrap().with_max_body_bytes(9);
    let error = client.send(get(url, HeaderMap::new())).await.unwrap_err();
    assert_eq!(error.kind, TransportErrorKind::BodyTooLarge);
    server.join().unwrap();

    let (exact_url, exact_server) = serve_chunked(&["hello", "world"]);
    let exact = ReqwestClient::new().unwrap().with_max_body_bytes(10);
    let response = exact.send(get(exact_url, HeaderMap::new())).await.unwrap();
    assert_eq!(response.body, "helloworld");
    exact_server.join().unwrap();
}

/// The default limit is `16 MiB`.
#[test]
fn the_default_body_limit_is_16_mib() {
    assert_eq!(ReqwestClient::DEFAULT_MAX_BODY_BYTES, 16 * 1024 * 1024);
}

/// A name, value or domain with `;`, CR, or LF sets no cookie.
#[test]
fn cookies_with_separators_are_rejected() {
    let client = ReqwestClient::new().unwrap();
    client.set_cookie("a;b", "1", ".youtube.com");
    client.set_cookie("a", "1; Path=/x", ".youtube.com");
    client.set_cookie("a", "1\r\nSet-Cookie: x=y", ".youtube.com");
    client.set_cookie("a\n", "1", ".youtube.com");
    client.set_cookie("a", "1", ".youtube.com; Secure");
    assert_eq!(client.cookies_for("https://www.youtube.com/"), None);

    client.set_cookie("ok", "fine", ".youtube.com");
    assert_eq!(
        client.cookies_for("https://www.youtube.com/"),
        Some("ok=fine".to_owned())
    );
}

/// `Debug` shows neither the proxy's credentials nor its URL.
#[test]
fn debug_hides_the_proxy_credentials() {
    let config =
        GenericProxyConfig::new(Some("http://user:hunter2@127.0.0.1:9".to_owned()), None).unwrap();
    let client = ReqwestClient::with_proxy(Some(&config)).unwrap();
    let shown = format!("{client:?}");
    assert!(!shown.contains("hunter2"), "{shown}");
    assert!(shown.contains("max_body_bytes"));
}

/// A Webshare password with reserved characters reaches the proxy intact.
#[tokio::test]
async fn webshare_credentials_with_reserved_characters_reach_the_proxy() {
    let proxy = MockServer::start().await;
    Mock::given(method("GET"))
        .and(header(
            "proxy-authorization",
            "Basic dXNlci1yb3RhdGU6cEBzczp3L3JkIz8lNDEgw7xuw68=",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string("authorised"))
        .mount(&proxy)
        .await;
    let address = proxy.address();
    let config = WebshareProxyConfig::new("user", "p@ss:w/rd#?%41 ünï")
        .with_domain_name(address.ip().to_string())
        .with_proxy_port(address.port());

    let client = ReqwestClient::with_proxy(Some(&config)).unwrap();
    let response = client
        .send(get(
            "http://www.youtube.com/watch".to_owned(),
            HeaderMap::new(),
        ))
        .await
        .unwrap();

    assert_eq!(response.body, "authorised");
}
