mod protocol;
mod request;
mod response;
mod tls;

use crate::protocol::Protocol;
use crate::response::{Response, Status};
use log::{error, info};
use std::io;
use std::{net, str};
use tokio::io::{copy, sink, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

const MAX_REQUEST_HEADER_SIZE: usize = 2048;
const TLS_LISTEN_PORT: u16 = 4443;
const TLS_CLIENT_CA_CERTIFICATE_PEM_FILENAME: &str = "ca.cert.pem";
const TLS_SERVER_CERTIFICATE_PEM_FILENAME: &str = "localhost.cert.pem";
const TLS_SERVER_PRIVATE_KEY_PEM_FILENAME: &str = "localhost.pem";
// const TLS_SERVER_CERTIFICATE_PEM_FILENAME: &str = "ruby.sh.fullchain.pem";
// const TLS_SERVER_PRIVATE_KEY_PEM_FILENAME: &str = "ruby.sh.pem";

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();

    let mut addr: net::SocketAddr = "[::]:443".parse().unwrap();
    addr.set_port(TLS_LISTEN_PORT);

    let config = tls::make_config(
        TLS_CLIENT_CA_CERTIFICATE_PEM_FILENAME,
        TLS_SERVER_CERTIFICATE_PEM_FILENAME,
        TLS_SERVER_PRIVATE_KEY_PEM_FILENAME,
    );

    let acceptor = TlsAcceptor::from(config);

    let listener = TcpListener::bind(&addr).await?;

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let acceptor = acceptor.clone();

        let fut = async move {
            let mut stream = acceptor.accept(stream).await?;

            let client_certificate_details =
                tls::extract_client_certificate_details_from_stream(&stream);

            let mut buf = [0u8; MAX_REQUEST_HEADER_SIZE];
            if stream.read(&mut buf[..]).await? == MAX_REQUEST_HEADER_SIZE {
                error!("Request from {}: request bigger than max size", peer_addr);
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "request bigger than max size",
                ));
            }

            let request = Protocol::parse_req_buf(
                peer_addr,
                &client_certificate_details,
                &buf,
                &mut stream,
            )
            .await;

            match request {
                Ok(request) => {
                    info!(
                        "OK [{} -> {}] [{}] [{}] {}",
                        peer_addr,
                        TLS_LISTEN_PORT,
                        request.protocol(),
                        request.client_certificate_details(),
                        request.path()
                    );

                    let response = Response::new(
                        Status::Success,
                        "text/plain",
                        format!("Hey, {}", client_certificate_details.common_name()).as_bytes(),
                    );
                    request
                        .protocol()
                        .write_response(response, &mut stream)
                        .await?;
                }
                Err(err) => {
                    error!("ERROR [{} -> {}] msg = {}", peer_addr, TLS_LISTEN_PORT, err);
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
