use std::fmt;

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
            Status::Success => write!(f, "Success"),
            Status::TemporaryRedirect => write!(f, "Redirecting..."),
            Status::PermanentRedirect => write!(f, "Redirecting..."),
            Status::Unauthenticated => write!(f, "Unauthenticated"),
            Status::Unauthorized => write!(f, "Unauthorized"),
            Status::NotFound => write!(f, "Not found"),
            Status::RequestTooLarge => write!(f, "Request too large"),
            Status::RateLimit => write!(f, "Rate limited"),
            Status::OtherServerError => write!(f, "Other server error"),
            Status::OtherClientError => write!(f, "Other client error"),
        }
    }
}

pub struct Response {
    status: Status,
    mime_type: String,
    body: Vec<u8>,
}

impl Response {
    pub fn new(status: Status, mime_type: &str, body: &[u8]) -> Response {
        Response {
            status: status,
            mime_type: mime_type.to_string(),
            body: body.to_vec(),
        }
    }

    pub fn new_for_status(status: Status) -> Response {
        Response {
            status: status,
            mime_type: "text/plain".to_string(),
            body: format!("{}", status).as_str().into(),
        }
    }
    
    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }
}