use crate::context::ServerContext;
use crate::protocol::Protocol;
use crate::templates::{Markup, TemplateRequestContext};
use crate::tls::ClientCertificateDetails;
use serde_json::json;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use url::Url;

pub struct Request {
    server_context: Arc<ServerContext>,
    peer_addr: SocketAddr,
    url: Url,
    client_certificate_details: ClientCertificateDetails,
    protocol: Protocol,
    template_context: TemplateRequestContext,
}

impl Request {
    pub fn new(
        server_context: Arc<ServerContext>,
        peer_addr: SocketAddr,
        url: Url,
        client_certificate_details: ClientCertificateDetails,
    ) -> Request {
        let protocol = match url.scheme() {
            "gemini" => Protocol::Gemini,
            _ => Protocol::Https,
        };

        let template_context = TemplateRequestContext {
            meta: json!({}),
            data: server_context.get_data(),
            posts: server_context.get_sorted_posts_for_protocol(protocol),
            peer_addr: peer_addr,
            path: (url.path()).to_string(),
            is_authenticated: !client_certificate_details.is_anonymous(),
            is_anonymous: client_certificate_details.is_anonymous(),
            common_name: client_certificate_details.common_name(),
            protocol: protocol,
            markup: Markup::default_for_protocol(protocol),
            is_gemini: protocol == Protocol::Gemini,
            is_https: protocol == Protocol::Https,
            os_platform: env::consts::OS.to_string(),
        };

        Request {
            server_context: server_context,
            peer_addr: peer_addr,
            url: url,
            client_certificate_details: client_certificate_details,
            protocol: protocol,
            template_context: template_context,
        }
    }

    pub fn server_context(&self) -> &Arc<ServerContext> {
        &self.server_context
    }

    pub fn peer_addr(&self) -> &SocketAddr {
        &self.peer_addr
    }

    pub fn client_certificate_details(&self) -> &ClientCertificateDetails {
        &self.client_certificate_details
    }

    pub fn path(&self) -> &str {
        self.url.path()
    }

    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    pub fn template_context(&self) -> &TemplateRequestContext {
        &self.template_context
    }

    pub fn mut_template_context(&mut self) -> &mut TemplateRequestContext {
        &mut self.template_context
    }
}
