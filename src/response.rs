use std::{fmt, str::FromStr};

use crate::{files::try_load_file_for_path, request::Request};

#[derive(Copy, Clone, PartialEq)]
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

#[derive(Debug, PartialEq, Eq)]
pub struct UnknownStatusError;

impl FromStr for Status {
    type Err = UnknownStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "success" => Ok(Status::Success),
            "temporary_redirect" => Ok(Status::TemporaryRedirect),
            "permanent_redirect" => Ok(Status::PermanentRedirect),
            "unauthenticated" => Ok(Status::Unauthenticated),
            "unauthorized" => Ok(Status::Unauthorized),
            "not_found" => Ok(Status::NotFound),
            "request_too_large" => Ok(Status::RequestTooLarge),
            "rate_limited" => Ok(Status::RateLimit),
            "other_server_error" => Ok(Status::OtherServerError),
            "other_client_error" => Ok(Status::OtherClientError),
            _ => Err(UnknownStatusError),
        }
    }
}

#[derive(Clone)]
pub struct Response {
    status: Status,
    media_type: String,
    redirect_uri: String,
    body: Vec<u8>,
    cacheable: bool,
}

impl Response {
    pub fn new(status: Status, media_type: &str, body: &[u8], cacheable: bool) -> Response {
        Response {
            status: status,
            media_type: media_type.to_string(),
            redirect_uri: "".to_string(),
            body: body.to_vec(),
            cacheable: cacheable,
        }
    }

    pub fn new_with_redirect_uri(status: Status, redirect_uri: &str) -> Response {
        Response {
            status: status,
            media_type: "".to_string(),
            redirect_uri: redirect_uri.to_string(),
            body: Vec::new(),
            cacheable: false,
        }
    }

    pub fn new_for_request_and_status(request: &mut Request, status: Status) -> Response {
        for try_ext in request.protocol().media_type_file_extensions() {
            let try_path = format!(
                "{}/{}.{}",
                request.server_context().config().errdocs_path(),
                status,
                try_ext
            );

            match try_load_file_for_path(&try_path, request) {
                Ok(response) => {
                    return Response {
                        status: status,
                        media_type: response.media_type().to_owned(),
                        redirect_uri: "".to_string(),
                        body: response.body().to_vec(),
                        cacheable: false,
                    }
                }
                Err(_) => {}
            }
        }

        Response {
            status: status,
            media_type: "text/plain".to_string(),
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
            }
            .into(),
            cacheable: false,
        }
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn cacheable(&self) -> bool {
        self.cacheable
    }
}
