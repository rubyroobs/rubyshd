use crate::protocol::Protocol;
use crate::tls::ClientCertificateDetails;
use std::net::SocketAddr;
use url::Url;

const DEFAULT_HOSTNAME: &str = "ruby.sh";

pub struct Request {
    peer_addr: SocketAddr,
    url: Url,
    client_certificate_details: ClientCertificateDetails,
}

impl Request {
    pub fn new(
        peer_addr: SocketAddr,
        url: Url,
        client_certificate_details: ClientCertificateDetails,
    ) -> Request {
        Request {
            peer_addr: peer_addr,
            url: url,
            client_certificate_details: client_certificate_details,
        }
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
            _ => Protocol::Http,
        }
    }
}
