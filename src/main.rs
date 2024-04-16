use httparse;
use log::{error, info};
use rustls::crypto::{aws_lc_rs as provider, CryptoProvider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::RootCertStore;
use std::io::{self, BufReader};
use std::sync::Arc;
use std::{fs, net, str};
use tokio::io::{copy, sink, split, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::{rustls, TlsAcceptor};
use url::Url;
use x509_parser::prelude::*;

const MAX_REQUEST_HEADER_SIZE: usize = 2048;
const TLS_CLIENT_CA_CERTIFICATE_PEM_FILENAME: &str = "ca.cert.pem";
const TLS_SERVER_CERTIFICATE_PEM_FILENAME: &str = "localhost.cert.pem";
const TLS_SERVER_PRIVATE_KEY_PEM_FILENAME: &str = "localhost.pem";
// const TLS_SERVER_CERTIFICATE_PEM_FILENAME: &str = "ruby.sh.fullchain.pem";
// const TLS_SERVER_PRIVATE_KEY_PEM_FILENAME: &str = "ruby.sh.pem";

struct Request {
    url: Url,
    client_common_name: String,
}

struct HttpHeaderEntry {
    name: String,
    value: String,
}

fn load_certs(filename: &str) -> Vec<CertificateDer<'static>> {
    let certfile = fs::File::open(filename).expect("cannot open certificate file");
    let mut reader = BufReader::new(certfile);
    rustls_pemfile::certs(&mut reader)
        .map(|result| result.unwrap())
        .collect()
}

fn load_private_key(filename: &str) -> PrivateKeyDer<'static> {
    let keyfile = fs::File::open(filename).expect("cannot open private key file");
    let mut reader = BufReader::new(keyfile);

    loop {
        match rustls_pemfile::read_one(&mut reader).expect("cannot parse private key .pem file") {
            Some(rustls_pemfile::Item::Pkcs1Key(key)) => return key.into(),
            Some(rustls_pemfile::Item::Pkcs8Key(key)) => return key.into(),
            Some(rustls_pemfile::Item::Sec1Key(key)) => return key.into(),
            None => break,
            _ => {}
        }
    }

    panic!(
        "no keys found in {:?} (encrypted keys not supported)",
        filename
    );
}

fn make_config() -> Arc<rustls::ServerConfig> {
    let client_root_certs = load_certs(TLS_CLIENT_CA_CERTIFICATE_PEM_FILENAME);
    let mut client_auth_roots = RootCertStore::empty();
    for root in client_root_certs {
        client_auth_roots.add(root).unwrap();
    }
    let client_auth = WebPkiClientVerifier::builder(client_auth_roots.into())
        .allow_unauthenticated()
        .build()
        .unwrap();

    let versions = rustls::ALL_VERSIONS.to_vec();
    let suites = provider::ALL_CIPHER_SUITES.to_vec();

    let certs = load_certs(TLS_SERVER_CERTIFICATE_PEM_FILENAME);
    let privkey = load_private_key(TLS_SERVER_PRIVATE_KEY_PEM_FILENAME);

    let mut config = rustls::ServerConfig::builder_with_provider(
        CryptoProvider {
            cipher_suites: suites,
            ..provider::default_provider()
        }
        .into(),
    )
    .with_protocol_versions(&versions)
    .expect("inconsistent cipher-suites/versions specified")
    .with_client_cert_verifier(client_auth)
    .with_single_cert(certs, privkey)
    .expect("bad certificates/private key");

    config.key_log = Arc::new(rustls::KeyLogFile::new());

    Arc::new(config)
}

fn safe_str(str: &str) -> &str {
    str.lines().next().unwrap_or("")
}

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();

    let mut addr: net::SocketAddr = "[::]:443".parse().unwrap();
    addr.set_port(4443);

    let config = make_config();

    let acceptor = TlsAcceptor::from(config);

    let listener = TcpListener::bind(&addr).await?;

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let acceptor = acceptor.clone();

        let fut =
            async move {
                let mut stream = acceptor.accept(stream).await?;

                let cert = match stream.get_ref().1.peer_certificates() {
                    Some(der_certs) => {
                        match der_certs.iter().next() {
                            Some(first_der_cert) => {
                                match parse_x509_certificate(first_der_cert) {
                                    Ok((_, cert)) => Some(cert),
                                    Err(_) => None
                                }
                            },
                            None => None
                        }
                    },
                    None => None
                };

                let common_name = match cert.clone() {
                    Some(cert_data) => match cert_data.subject().iter_common_name().next() {
                        Some(cn) => match cn.as_str() {
                            Ok(cn_str) => Some(cn_str.to_string()),
                            Err(_) => None,
                        },
                        None => None
                    },
                    None => None
                };
                
                // if let Some(der_certs) = stream.get_ref().1.peer_certificates() {
                //     if let Some(der_cert) = der_certs.iter().next() {
                //         if let Ok((_, parsed_cert)) = parse_x509_certificate(der_cert) {
                //             let cloned_cert = parsed_cert.clone();
                //             if let Some(first) = cloned_cert.subject().iter_common_name().next() {
                //                 if let Ok(parsed_cn) = first.as_str() {
                //                     cn = Some(parsed_cn);
                //                 }
                //             }
                //         }
                //     }
                // }

                let mut buf = [0u8; MAX_REQUEST_HEADER_SIZE];
                let n = stream.read(&mut buf[..]).await?;
                if n == MAX_REQUEST_HEADER_SIZE {
                    error!("Request from {}: request bigger than max size", peer_addr);
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "request bigger than max size",
                    ));
                }

                let mut output = sink();

                match buf {
                    buf if buf.starts_with(b"gemini:") => {
                        // Gemini
                        println!("hello gemini! uri: {}", std::str::from_utf8(&buf).unwrap());
                        stream
                            .write_all(
                                &b"20 text/gemini\r\n\
                        Hello world!"[..],
                            )
                            .await?;
                    }
                    _ => {
                        // HTTP
                        let mut headers = [httparse::EMPTY_HEADER; 16];
                        let mut r = httparse::Request::new(&mut headers);
                        let status = httparse::ParserConfig::default()
                            .parse_request(&mut r, &buf)
                            .map_err(|e| {
                                let msg = format!("failed to parse http request: {:?}", e);
                                io::Error::new(io::ErrorKind::Other, msg)
                            })?;

                        let mut headers: Vec<HttpHeaderEntry> = Vec::new();
                        let mut body: Vec<u8> = Vec::new();
    
                        let (status, reason) = match status {
                            httparse::Status::Complete(_) => {
                                body.extend_from_slice(
                                    format!("Hey, {}", common_name.unwrap_or("anonymous".to_string())).clone().as_bytes()
                                );

                                (
                                    200,
                                    "OK"
                                )
                            },
                            httparse::Status::Partial => {
                                (
                                    413,
                                    "Content Too Large"
                                )
                            },
                        };

                        let body_len = body.len().to_string();

                        // Default headers
                        headers.push(
                            HttpHeaderEntry {
                                name: "Content-Length".to_string(),
                                value: body_len,
                            }
                        );

                        headers.push(
                            HttpHeaderEntry {
                                name: "Server".to_string(),
                                value: "rubyshd".to_string(),
                            }
                        );

                        // Headers
                        stream.write_all(&b"HTTP/1.1 "[..]).await?;
                        stream.write_all(status.to_string().as_bytes()).await?;
                        stream.write_all(&b" "[..]).await?;
                        stream.write_all(safe_str(reason).as_bytes()).await?;
                        stream.write_all(&b"\r\n"[..]).await?;

                        for entry in headers {
                            stream.write_all(safe_str(&entry.name).as_bytes()).await?;
                            stream.write_all(&b": "[..]).await?;
                            stream.write_all(safe_str(&entry.value).as_bytes()).await?;
                            stream.write_all(&b"\r\n"[..]).await?;    
                        }

                        stream.write_all(&b"\r\n"[..]).await?;

                        // Body
                        stream.write_all(&body).await?;

                        stream.write_all(&b"\r\n"[..]).await?;
                    }
                }

                stream.shutdown().await?;
                copy(&mut stream, &mut output).await?;

                info!("Request from {}: OK", peer_addr);

                Ok(()) as io::Result<()>
            };

        tokio::spawn(async move {
            if let Err(err) = fut.await {
                eprintln!("{:?}", err);
            }
        });
    }
}
