use rustls::crypto::{aws_lc_rs as provider, CryptoProvider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::RootCertStore;
use std::io::BufReader;
use std::sync::Arc;
use std::{fmt, fs, str};
use tokio::net::TcpStream;
use tokio_rustls::rustls;
use tokio_rustls::server::TlsStream;
use x509_parser::prelude::*;

use crate::config::Config;

#[derive(Clone)]
pub struct ClientCertificateDetails {
    common_name: Option<String>,
}

impl fmt::Display for ClientCertificateDetails {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            self.common_name.clone().unwrap_or("anonymous".to_string())
        )
    }
}

impl ClientCertificateDetails {
    pub fn new_anonymous() -> ClientCertificateDetails {
        ClientCertificateDetails { common_name: None }
    }

    pub fn is_anonymous(&self) -> bool {
        self.common_name.is_none()
    }

    pub fn common_name(&self) -> String {
        match self.common_name.clone() {
            Some(cn) => cn,
            None => "anonymous".to_string(),
        }
    }
}

pub fn extract_client_certificate_details_from_stream(
    stream: &TlsStream<TcpStream>,
) -> ClientCertificateDetails {
    let cert = match stream.get_ref().1.peer_certificates() {
        Some(der_certs) => match der_certs.iter().next() {
            Some(first_der_cert) => match parse_x509_certificate(first_der_cert) {
                Ok((_, cert)) => Some(cert),
                Err(_) => None,
            },
            None => None,
        },
        None => None,
    };

    let details = match cert.clone() {
        Some(cert_data) => match cert_data.subject().iter_common_name().next() {
            Some(cn) => match cn.as_str() {
                Ok(cn_str) => Some(ClientCertificateDetails {
                    common_name: Some(cn_str.to_string()),
                }),
                Err(_) => None,
            },
            None => None,
        },
        None => None,
    };

    details.unwrap_or(ClientCertificateDetails::new_anonymous())
}

pub fn make_config(config: &Config) -> Arc<rustls::ServerConfig> {
    let client_root_certs = load_certs(config.tls_client_ca_certificate_pem_filename());
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

    let certs = load_certs(config.tls_server_certificate_pem_filename());
    let privkey = load_private_key(config.tls_server_private_key_pem_filename());

    let mut server_config = rustls::ServerConfig::builder_with_provider(
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

    server_config.key_log = Arc::new(rustls::KeyLogFile::new());

    Arc::new(server_config)
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
