use std::fmt;

use crate::{files::try_load_files_with_template,request::Request, ERRDOCS_PATH};

#[derive(Copy, Clone)]
pub enum Status {
    Success,
    TemporaryRedirect,
    PermanentRedirect,
    Unauthenticated,
    Unauthorized,
    NotFound,
    RequestTooLarge,
    RateLimit,
    OtherServerError,
    OtherClientError,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Status::Success => write!(f, "success"),
            Status::TemporaryRedirect => write!(f, "temporary_redirect"),
            Status::PermanentRedirect => write!(f, "permanent_redirect"),
            Status::Unauthenticated => write!(f, "unauthenticated"),
            Status::Unauthorized => write!(f, "unauthorized"),
            Status::NotFound => write!(f, "not_found"),
            Status::RequestTooLarge => write!(f, "request_too_large"),
            Status::RateLimit => write!(f, "rate_limited"),
            Status::OtherServerError => write!(f, "other_server_error"),
            Status::OtherClientError => write!(f, "other_client_error"),
        }
    }
}

pub struct Response {
    status: Status,
    mime_type: String,
    redirect_uri: String,
    body: Vec<u8>,
}

impl Response {
    pub fn new(status: Status, mime_type: &str, body: &[u8]) -> Response {
        Response {
            status: status,
            mime_type: mime_type.to_string(),
            redirect_uri: "".to_string(),
            body: body.to_vec(),
        }
    }

    pub fn new_for_request_and_status(request: &Request, status: Status) -> Response {
        for try_ext in request.protocol().mime_file_extensions() {
            let try_path = format!("{}/{}.{}", ERRDOCS_PATH, status, try_ext);
    
            match try_load_files_with_template(&try_path, &request) {
                Ok(body) => {
                    return Response {
                        status: status,
                        mime_type: mime_guess::from_ext(&try_ext).first_or_text_plain().essence_str().to_string(),
                        redirect_uri: "".to_string(),
                        body: body,
                    }
                },
                Err(_) => {}
            }
        }

        Response {
            status: status,
            mime_type: "text/plain".to_string(),
            redirect_uri: "".to_string(),
            body: match status {
                Status::Success => "Success",
                Status::TemporaryRedirect => "Temporary redirect",
                Status::PermanentRedirect => "Permanent redirect",
                Status::Unauthenticated => "Unauthenticated",
                Status::Unauthorized => "Unauthorized",
                Status::NotFound => "Not found",
                Status::RequestTooLarge => "Request too large",
                Status::RateLimit => "Rate limited",
                Status::OtherServerError => "Other server error",
                Status::OtherClientError => "Other client error",
            }.into(),
        }
    }
    
    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }

    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }
}