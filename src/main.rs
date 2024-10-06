mod config;
mod files;
mod protocol;
mod request;
mod response;
mod router;
mod templates;
mod tls;

use crate::protocol::Protocol;
use config::Config;
use log::{debug, error};
use router::route_request;
use std::sync::Arc;
use std::{io, net};
use tokio::io::{copy, sink, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

#[cfg(target_os = "openbsd")]
use openbsd::unveil;

#[cfg(target_os = "openbsd")]
pub fn setup_unveil(server_config: Arc<Config>) {
    debug!("openbsd, calling unveil");
    unveil("/dev/urandom", "r").expect("could not unveil urandom");
    unveil(server_config.public_root_path(), "r").expect("could not unveil public data folder");
    unveil(server_config.errdocs_path(), "r").expect("could not unveil errors data folder");
    unveil(server_config.tls_client_ca_certificate_pem_filename(), "r")
        .expect("could not unveil TLS CA certificate");
    unveil(server_config.tls_server_certificate_pem_filename(), "r")
        .expect("could not unveil TLS server certificate");
    unveil(server_config.tls_server_private_key_pem_filename(), "r")
        .expect("could not unveil TLS server private key");

    unveil::disable();
}

#[cfg(not(target_os = "openbsd"))]
pub fn setup_unveil() {
    debug!("not openbsd. :(");
}

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();

    let server_config = Arc::new(Config::new_from_env());

    debug!("Starting server with config: {:#?}", server_config);
    setup_unveil();

    let mut addr: net::SocketAddr = "[::]:443".parse().unwrap();
    addr.set_port(server_config.tls_listen_port());

    let tls_config = tls::make_config(&server_config);

    let acceptor = TlsAcceptor::from(tls_config);

    let listener = TcpListener::bind(&addr).await?;

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let server_config = server_config.clone();
        let tls_listen_port = server_config.tls_listen_port();

        let fut = async move {
            let mut stream = acceptor.accept(stream).await?;

            let client_certificate_details =
                tls::extract_client_certificate_details_from_stream(&stream);

            let mut buf = vec![0u8; server_config.max_request_header_size()];
            if stream.read(&mut buf[..]).await? == server_config.max_request_header_size() {
                error!("Request from {}: request bigger than max size", peer_addr);
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "request bigger than max size",
                ));
            }

            let request = Protocol::parse_req_buf(
                server_config,
                peer_addr,
                &client_certificate_details,
                &buf,
                &mut stream,
            )
            .await;

            match request {
                Ok(request) => {
                    let response = route_request(&request);

                    request
                        .protocol()
                        .write_response(response, &mut stream)
                        .await?;
                }
                Err(err) => {
                    error!("ERROR [{} -> {}] msg = {}", peer_addr, tls_listen_port, err);
                }
            }

            stream.shutdown().await?;

            let mut output = sink();
            copy(&mut stream, &mut output).await?;

            Ok(()) as io::Result<()>
        };

        tokio::spawn(async move {
            if let Err(err) = fut.await {
                eprintln!("{:?}", err);
            }
        });
    }
}
