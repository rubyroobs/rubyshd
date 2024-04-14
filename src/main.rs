use std::collections::HashMap;
use std::io::{self, BufReader, Read, Write};
use std::{slice, str, fmt};
use std::sync::Arc;
use bytes::BytesMut;
use std::{fs, net};
use log::{debug, error, info};
use tokio::io::{copy, sink, split, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::{rustls, TlsAcceptor};
use rustls::crypto::{aws_lc_rs as provider, CryptoProvider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConnection};
use x509_parser::prelude::*;
use url::{Url};
use httparse;

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

        let fut = async move {
            let mut stream = acceptor.accept(stream).await?;

            let cn = match stream.get_ref().1.peer_certificates() {
                Some(certs) => {
                    certs.iter().next().and_then(|cert| {
                        match parse_x509_certificate(cert) {
                            Ok((e, parsed_cert)) => {
                                parsed_cert
                                    .subject()
                                    .iter_common_name()
                                    .next()
                                    .and_then(|cn| {
                                        match cn.as_str() {
                                            Ok(cn) => Some(cn.to_owned()),
                                            _ => None
                                        }
                                    })
                            },
                            Err(err) => None
                        }
                    })
                },
                None => None
            };

            let mut buf = [0u8; MAX_REQUEST_HEADER_SIZE];
            let n = stream.read(&mut buf[..]).await?;
            if n == MAX_REQUEST_HEADER_SIZE {
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
                    let status  = httparse::ParserConfig::default()
                        .parse_request(&mut r, &buf)
                        .map_err(|e| {
                            let msg = format!("failed to parse http request: {:?}", e);
                            io::Error::new(io::ErrorKind::Other, msg)
                        })?;
                    let amt = match status {
                        httparse::Status::Complete(amt) => amt,
                        httparse::Status::Partial => 0,
                    };
        
                    let response = format!("Hey, {}", cn.unwrap_or("anonymous".to_string()));

                    // Headers
                    stream.write_all(&b"HTTP/1.1 200 OK\r\n"[..]).await?;
                    stream.write_all(&b"Content-Length: "[..]).await?;
                    stream.write_all(response.len().to_string().as_bytes()).await?;
                    stream.write_all(&b"\r\n"[..]).await?;

                    stream.write_all(&b"\r\n"[..]).await?;

                    // Body
                    stream.write_all(response.as_bytes()).await?;

                    stream.write_all(&b"\r\n"[..]).await?;
                }
            }

            stream.shutdown().await?;
            copy(&mut stream, &mut output).await?;

            println!("Hello: {}", peer_addr);
        
            Ok(()) as io::Result<()>
        };

        tokio::spawn(async move {
            if let Err(err) = fut.await {
                eprintln!("{:?}", err);
            }
        });
    }
}