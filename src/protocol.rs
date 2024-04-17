use crate::request::Request;
use crate::response::{Response, Status};
use crate::tls::ClientCertificateDetails;
use std::fmt;
use std::io::Error;
use std::net::SocketAddr;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use url::Url;

const DEFAULT_HOSTNAME: &str = "ruby.sh";

struct HttpHeaderEntry {
    name: String,
    value: String,
}

fn newline_stripped_safe_str(str: &str) -> &str {
    str.lines().next().unwrap_or("")
}

pub enum Protocol {
    Gemini,
    Http,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Protocol::Gemini => write!(f, "Gemini"),
            Protocol::Http => write!(f, "HTTP"),
        }
    }
}

impl Protocol {
    pub async fn write_response(
        &self,
        response: Response,
        stream: &mut TlsStream<TcpStream>,
    ) -> Result<(), Error> {
        match self {
            Protocol::Gemini => {
                let status_code = match response.status() {
                    Status::Success => 20,
                    Status::TemporaryRedirect => 30,
                    Status::PermanentRedirect => 31,
                    Status::Unauthenticated => 60,
                    Status::Unauthorized => 61,
                    Status::NotFound => 51,
                    Status::RequestTooLarge => 59,
                    Status::RateLimit => 44,
                    Status::OtherServerError => 40,
                    Status::OtherClientError => 59,
                };

                stream.write_all(status_code.to_string().as_bytes()).await?;
                stream.write_all(&b" "[..]).await?;
                stream
                    .write_all(newline_stripped_safe_str(response.mime_type()).as_bytes())
                    .await?;
                stream.write_all(&b"\r\n"[..]).await?;
                stream.write_all(response.body()).await?;
            }
            Protocol::Http => {
                let (status, reason) = match response.status() {
                    Status::Success => (200, "OK"),
                    Status::TemporaryRedirect => (302, "Moved Permanently"),
                    Status::PermanentRedirect => (301, "Found"),
                    Status::Unauthenticated => (401, "Unauthorized"),
                    Status::Unauthorized => (403, "Forbidden"),
                    Status::NotFound => (404, "Not Found"),
                    Status::RequestTooLarge => (413, "Payload Too Large"),
                    Status::RateLimit => (429, "Too Many Requests"),
                    Status::OtherServerError => (500, "OK"),
                    Status::OtherClientError => (400, "Internal Server Error"),
                };

                let body_len = response.body().len().to_string();

                let mut headers: Vec<HttpHeaderEntry> = Vec::new();

                // Default headers
                headers.push(HttpHeaderEntry {
                    name: "Content-Length".to_string(),
                    value: body_len,
                });

                headers.push(HttpHeaderEntry {
                    name: "Content-Type".to_string(),
                    value: response.mime_type().to_string(),
                });

                headers.push(HttpHeaderEntry {
                    name: "Server".to_string(),
                    value: "rubyshd".to_string(),
                });

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
                        Protocol::Gemini
                            .write_response(
                                Response::new_for_status(Status::OtherClientError),
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
                        Protocol::Gemini
                            .write_response(
                                Response::new_for_status(Status::OtherClientError),
                                stream,
                            )
                            .await;
                        return Err(format!("error parsing gemini url: {}", e));
                    }
                };

                Ok(Request::new(
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
                        Protocol::Http
                            .write_response(
                                Response::new_for_status(Status::OtherClientError),
                                stream,
                            )
                            .await;
                        return Err(format!("error parsing http request: {}", e));
                    }
                };

                match status {
                    httparse::Status::Complete(_) => (),
                    httparse::Status::Partial => {
                        Protocol::Http
                            .write_response(
                                Response::new_for_status(Status::RequestTooLarge),
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
                        Err(e) => DEFAULT_HOSTNAME.to_string(),
                    },
                    None => DEFAULT_HOSTNAME.to_string(),
                };

                let url = match Url::parse(
                    format!("https://{}{}", hostname, path).as_str(),
                ) {
                    Ok(url) => url,
                    Err(e) => {
                        Protocol::Http
                            .write_response(
                                Response::new_for_status(Status::OtherClientError),
                                stream,
                            )
                            .await;
                        return Err(format!("error converting http req to a url: {}", e));
                    }
                };

                Ok(Request::new(
                    peer_addr,
                    url,
                    client_certificate_details.clone(),
                ))
            }
        }
    }
}
