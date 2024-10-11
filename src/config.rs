use std::{env, net, path::PathBuf};

const DEFAULT_PUBLIC_ROOT_PATH: &str = "public_root";
const DEFAULT_PARTIALS_PATH: &str = "partials";
const DEFAULT_DATA_PATH: &str = "data";
const DEFAULT_ERRDOCS_PATH: &str = "errdocs";
const DEFAULT_MAX_REQUEST_HEADER_SIZE: usize = 2048;
const DEFAULT_TLS_LISTEN_BIND: &str = "127.0.0.1:4443";
const DEFAULT_TLS_CLIENT_CA_CERTIFICATE_PEM_FILENAME: &str = "ca.cert.pem";
const DEFAULT_TLS_SERVER_CERTIFICATE_PEM_FILENAME: &str = "localhost.cert.pem";
const DEFAULT_TLS_SERVER_PRIVATE_KEY_PEM_FILENAME: &str = "localhost.pem";
const DEFAULT_DEFAULT_HOSTNAME: &str = "localhost";

#[derive(Clone, Debug)]
pub struct Config {
    public_root_path: String,
    partials_path: String,
    data_path: String,
    errdocs_path: String,
    max_request_header_size: usize,
    tls_listen_bind: net::SocketAddrV4,
    tls_client_ca_certificate_pem_filename: String,
    tls_server_certificate_pem_filename: String,
    tls_server_private_key_pem_filename: String,
    default_hostname: String,
}

impl Config {
    pub fn new_from_env() -> Config {
        let public_root_path = check_directory_path(
            &env::var("PUBLIC_ROOT_PATH").unwrap_or(DEFAULT_PUBLIC_ROOT_PATH.into()),
        )
        .expect("Invalid PUBLIC_ROOT_PATH")
        .to_string();

        let partials_path = check_directory_path(
            &env::var("PARTIALS_PATH").unwrap_or(DEFAULT_PARTIALS_PATH.into()),
        )
        .expect("Invalid PARTIALS_PATH")
        .to_string();

        let data_path =
            check_directory_path(&env::var("DATA_PATH").unwrap_or(DEFAULT_DATA_PATH.into()))
                .expect("Invalid DATA_PATH")
                .to_string();

        let errdocs_path =
            check_directory_path(&env::var("ERRDOCS_PATH").unwrap_or(DEFAULT_ERRDOCS_PATH.into()))
                .expect("Invalid ERRDOCS_PATH")
                .to_string();

        let max_request_header_size: usize = env::var("MAX_REQUEST_HEADER_SIZE")
            .unwrap_or(format!("{}", DEFAULT_MAX_REQUEST_HEADER_SIZE))
            .parse()
            .expect("Invalid MAX_REQUEST_HEADER_SIZE");

        let tls_listen_bind: net::SocketAddrV4 = env::var("TLS_LISTEN_BIND")
            .unwrap_or(DEFAULT_TLS_LISTEN_BIND.to_string())
            .parse()
            .expect("Invalid TLS_LISTEN_BIND");

        let tls_client_ca_certificate_pem_filename = check_file_path(
            &env::var("TLS_CLIENT_CA_CERTIFICATE_PEM_FILENAME")
                .unwrap_or(DEFAULT_TLS_CLIENT_CA_CERTIFICATE_PEM_FILENAME.into()),
        )
        .expect("Invalid TLS_CLIENT_CA_CERTIFICATE_PEM_FILENAME")
        .to_string();

        let tls_server_certificate_pem_filename = check_file_path(
            &env::var("TLS_SERVER_CERTIFICATE_PEM_FILENAME")
                .unwrap_or(DEFAULT_TLS_SERVER_CERTIFICATE_PEM_FILENAME.into()),
        )
        .expect("Invalid TLS_SERVER_CERTIFICATE_PEM_FILENAME")
        .to_string();

        let tls_server_private_key_pem_filename = check_file_path(
            &env::var("TLS_SERVER_PRIVATE_KEY_PEM_FILENAME")
                .unwrap_or(DEFAULT_TLS_SERVER_PRIVATE_KEY_PEM_FILENAME.into()),
        )
        .expect("Invalid TLS_SERVER_PRIVATE_KEY_PEM_FILENAME")
        .to_string();

        let default_hostname =
            env::var("DEFAULT_HOSTNAME").unwrap_or(DEFAULT_DEFAULT_HOSTNAME.into());

        Config {
            public_root_path: public_root_path.into(),
            partials_path: partials_path.into(),
            data_path: data_path.into(),
            errdocs_path: errdocs_path.into(),
            max_request_header_size: max_request_header_size,
            tls_listen_bind: tls_listen_bind,
            tls_client_ca_certificate_pem_filename: tls_client_ca_certificate_pem_filename.into(),
            tls_server_certificate_pem_filename: tls_server_certificate_pem_filename.into(),
            tls_server_private_key_pem_filename: tls_server_private_key_pem_filename.into(),
            default_hostname: default_hostname,
        }
    }

    pub fn public_root_path(&self) -> &str {
        &self.public_root_path
    }

    pub fn partials_path(&self) -> &str {
        &self.partials_path
    }

    pub fn data_path(&self) -> &str {
        &self.data_path
    }

    pub fn errdocs_path(&self) -> &str {
        &self.errdocs_path
    }

    pub fn max_request_header_size(&self) -> usize {
        self.max_request_header_size
    }

    pub fn tls_listen_bind(&self) -> &net::SocketAddrV4 {
        &self.tls_listen_bind
    }

    pub fn tls_client_ca_certificate_pem_filename(&self) -> &str {
        &self.tls_client_ca_certificate_pem_filename
    }

    pub fn tls_server_certificate_pem_filename(&self) -> &str {
        &self.tls_server_certificate_pem_filename
    }

    pub fn tls_server_private_key_pem_filename(&self) -> &str {
        &self.tls_server_private_key_pem_filename
    }

    pub fn default_hostname(&self) -> &str {
        &self.default_hostname
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PathError;

fn check_file_path(path: &str) -> Result<String, PathError> {
    check_path(path, false)
}

fn check_directory_path(path: &str) -> Result<String, PathError> {
    check_path(path, true)
}

fn check_path(path: &str, is_directory: bool) -> Result<String, PathError> {
    let buf = PathBuf::from(path);

    if (buf.is_file() && !is_directory) || (buf.is_dir() && is_directory) {
        return match buf.canonicalize() {
            Ok(path) => match path.to_str() {
                Some(path_str) => Ok(path_str.to_owned()),
                None => Err(PathError),
            },
            Err(_) => Err(PathError),
        };
    }

    Err(PathError)
}
