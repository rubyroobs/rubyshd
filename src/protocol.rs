use crate::context::ServerContext;
use crate::request::Request;
use crate::response::{Response, Status};
use crate::tls::ClientCertificateDetails;
use std::fmt;
use std::io::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use url::Url;

const CACHEABLE_MAX_AGE_SECONDS: u16 = 14_400;

struct HttpHeaderEntry {
    name: String,
    value: String,
}

fn newline_stripped_safe_str(str: &str) -> &str {
    str.lines().next().unwrap_or("")
}

#[derive(PartialEq)]
pub enum Protocol {
    Gemini,
    Https,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Protocol::Gemini => write!(f, "Gemini"),
            Protocol::Https => write!(f, "HTTPS"),
        }
    }
}

impl Protocol {
    pub fn media_type(&self) -> String {
        match self {
            Protocol::Gemini => "text/gemini".into(),
            Protocol::Https => "text/html; charset=utf-8".into(),
        }
    }

    pub fn media_type_file_extensions(&self) -> Vec<String> {
        match self {
            Protocol::Gemini => vec!["gmi".into()],
            Protocol::Https => vec!["html".into(), "htm".into()],
        }
    }

    pub async fn write_response(
        &self,
        response: Response,
        stream: &mut TlsStream<TcpStream>,
    ) -> Result<(), Error> {
        match self {
            Protocol::Gemini => {
                let (status, prompt_content_type_uri_or_error) = match response.status() {
                    Status::Success => (20, response.media_type()),
                    Status::TemporaryRedirect => (30, response.redirect_uri()),
                    Status::PermanentRedirect => (31, response.redirect_uri()),
                    Status::Unauthenticated => (60, "Unauthorized"),
                    Status::Unauthorized => (61, "Forbidden"),
                    Status::NotFound => (51, "Not Found"),
                    Status::RequestTooLarge => (59, "Payload Too Large"),
                    Status::RateLimit => (44, "Too Many Requests"),
                    Status::OtherServerError => (40, "Internal Server Error"),
                    Status::OtherClientError => (59, "Bad Request"),
                };

                stream.write_all(status.to_string().as_bytes()).await?;
                stream.write_all(&b" "[..]).await?;
                stream
                    .write_all(
                        newline_stripped_safe_str(prompt_content_type_uri_or_error).as_bytes(),
                    )
                    .await?;
                stream.write_all(&b"\r\n"[..]).await?;

                // only write body if it's a 20
                if status == 20 {
                    stream.write_all(response.body()).await?;
                }
            }
            Protocol::Https => {
                let (status, reason) = match response.status() {
                    Status::Success => (200, "OK"),
                    Status::PermanentRedirect => (301, "Moved Permanently"),
                    Status::TemporaryRedirect => (302, "Found"),
                    Status::OtherClientError => (400, "Bad Request"),
                    Status::Unauthenticated => (401, "Unauthenticated"), // this is intentionally not "Unauthorized"
                    Status::Unauthorized => (403, "Forbidden"),
                    Status::NotFound => (404, "Not Found"),
                    Status::RequestTooLarge => (413, "Payload Too Large"),
                    Status::RateLimit => (429, "Too Many Requests"),
                    Status::OtherServerError => (500, "Internal Server Error"),
                };

                let body_len = response.body().len();

                let mut headers: Vec<HttpHeaderEntry> = Vec::new();

                // Default headers
                headers.push(HttpHeaderEntry {
                    name: "Content-Length".to_string(),
                    value: body_len.to_string(),
                });

                if body_len > 0 {
                    headers.push(HttpHeaderEntry {
                        name: "Content-Type".to_string(),
                        value: response.media_type().to_string(),
                    });

                    let cache_max_age = match response.cacheable() {
                        true => CACHEABLE_MAX_AGE_SECONDS,
                        false => 0,
                    };

                    headers.push(HttpHeaderEntry {
                        name: "Cache-Control".to_string(),
                        value: format!("public, max-age={}, must-revalidate", cache_max_age),
                    });
                }

                headers.push(HttpHeaderEntry {
                    name: "Server".to_string(),
                    value: "rubyshd".to_string(),
                });

                if status == 301 || status == 302 {
                    headers.push(HttpHeaderEntry {
                        name: "Location".to_string(),
                        value: response.redirect_uri().to_string(),
                    });
                }

                // Headers
                stream.write_all(&b"HTTP/1.1 "[..]).await?;
                stream.write_all(status.to_string().as_bytes()).await?;
                stream.write_all(&b" "[..]).await?;
                stream
                    .write_all(newline_stripped_safe_str(reason).as_bytes())
                    .await?;
                stream.write_all(&b"\r\n"[..]).await?;

                for entry in headers {
                    stream
                        .write_all(newline_stripped_safe_str(&entry.name).as_bytes())
                        .await?;
                    stream.write_all(&b": "[..]).await?;
                    stream
                        .write_all(newline_stripped_safe_str(&entry.value).as_bytes())
                        .await?;
                    stream.write_all(&b"\r\n"[..]).await?;
                }

                stream.write_all(&b"\r\n"[..]).await?;

                // Body
                stream.write_all(response.body()).await?;

                stream.write_all(&b"\r\n"[..]).await?;
            }
        }

        Ok(())
    }

    pub async fn parse_req_buf(
        server_context: Arc<ServerContext>,
        peer_addr: SocketAddr,
        client_certificate_details: &ClientCertificateDetails,
        buf: &[u8],
        stream: &mut TlsStream<TcpStream>,
    ) -> Result<Request, String> {
        match buf {
            buf if buf.starts_with(b"gemini:") => {
                // gemini:... are gemini requests
                let raw_url = match std::str::from_utf8(buf) {
                    Ok(buf_str) => buf_str.lines().next().unwrap(),
                    Err(e) => {
                        let _ = Protocol::Gemini
                            .write_response(
                                Response::new_for_request_and_status(
                                    &Request::new(
                                        server_context,
                                        peer_addr,
                                        Url::parse("gemini://localhost/").unwrap(),
                                        client_certificate_details.clone(),
                                    ),
                                    Status::OtherClientError,
                                ),
                                stream,
                            )
                            .await;
                        return Err(format!(
                            "request looks like gemini but is not a valid UTF-8 seq: {}",
                            e
                        ));
                    }
                };

                let url = match Url::parse(raw_url) {
                    Ok(url) => url,
                    Err(e) => {
                        let _ = Protocol::Gemini
                            .write_response(
                                Response::new_for_request_and_status(
                                    &Request::new(
                                        server_context,
                                        peer_addr,
                                        Url::parse("gemini://localhost/").unwrap(),
                                        client_certificate_details.clone(),
                                    ),
                                    Status::OtherClientError,
                                ),
                                stream,
                            )
                            .await;
                        return Err(format!("error parsing gemini url: {}", e));
                    }
                };

                Ok(Request::new(
                    server_context,
                    peer_addr,
                    url,
                    client_certificate_details.clone(),
                ))
            }
            _ => {
                // HTTP
                let mut headers = [httparse::EMPTY_HEADER; 16];
                let mut r = httparse::Request::new(&mut headers);
                let status = match httparse::ParserConfig::default().parse_request(&mut r, &buf) {
                    Ok(status) => status,
                    Err(e) => {
                        let _ = Protocol::Https
                            .write_response(
                                Response::new_for_request_and_status(
                                    &Request::new(
                                        server_context,
                                        peer_addr,
                                        Url::parse("https://localhost/").unwrap(),
                                        client_certificate_details.clone(),
                                    ),
                                    Status::OtherClientError,
                                ),
                                stream,
                            )
                            .await;
                        return Err(format!("error parsing http request: {}", e));
                    }
                };

                match status {
                    httparse::Status::Complete(_) => (),
                    httparse::Status::Partial => {
                        let _ = Protocol::Https
                            .write_response(
                                Response::new_for_request_and_status(
                                    &Request::new(
                                        server_context,
                                        peer_addr,
                                        Url::parse("https://localhost/").unwrap(),
                                        client_certificate_details.clone(),
                                    ),
                                    Status::RequestTooLarge,
                                ),
                                stream,
                            )
                            .await;
                        return Err("http request is too large".to_string());
                    }
                };

                let path = r.path.unwrap_or("/").to_string();

                let hostname = match headers
                    .iter()
                    .find(|h| h.name.to_ascii_uppercase() == "HOST")
                {
                    Some(header) => match String::from_utf8(header.value.to_vec()) {
                        Ok(buf_str) => buf_str,
                        Err(_) => server_context.config().default_hostname().to_string(),
                    },
                    None => server_context.config().default_hostname().to_string(),
                };

                let url = match Url::parse(format!("https://{}{}", hostname, path).as_str()) {
                    Ok(url) => url,
                    Err(e) => {
                        let _ = Protocol::Https
                            .write_response(
                                Response::new_for_request_and_status(
                                    &Request::new(
                                        server_context,
                                        peer_addr,
                                        Url::parse("https://localhost/").unwrap(),
                                        client_certificate_details.clone(),
                                    ),
                                    Status::OtherClientError,
                                ),
                                stream,
                            )
                            .await;
                        return Err(format!("error converting http req to a url: {}", e));
                    }
                };

                Ok(Request::new(
                    server_context,
                    peer_addr,
                    url,
                    client_certificate_details.clone(),
                ))
            }
        }
    }
}
