use crate::context::ServerContext;
use crate::protocol::Protocol;
use crate::tls::ClientCertificateDetails;
use std::net::SocketAddr;
use std::sync::Arc;
use url::Url;

pub struct Request {
    server_context: Arc<ServerContext>,
    peer_addr: SocketAddr,
    url: Url,
    client_certificate_details: ClientCertificateDetails,
}

impl Request {
    pub fn new(
        server_context: Arc<ServerContext>,
        peer_addr: SocketAddr,
        url: Url,
        client_certificate_details: ClientCertificateDetails,
    ) -> Request {
        Request {
            server_context: server_context,
            peer_addr: peer_addr,
            url: url,
            client_certificate_details: client_certificate_details,
        }
    }

    pub fn server_context(&self) -> &Arc<ServerContext> {
        &self.server_context
    }

    pub fn peer_addr(&self) -> &SocketAddr {
        &self.peer_addr
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    pub fn client_certificate_details(&self) -> &ClientCertificateDetails {
        &self.client_certificate_details
    }

    pub fn path(&self) -> &str {
        self.url.path()
    }

    pub fn protocol(&self) -> Protocol {
        match self.url.scheme() {
            "gemini" => Protocol::Gemini,
            _ => Protocol::Https,
        }
    }
}
