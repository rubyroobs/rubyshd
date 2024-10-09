use log::error;

use crate::request::Request;
use crate::response::{Response, Status};
use crate::templates::render_response_body_for_request;
use std::path::PathBuf;

pub fn try_load_files_with_template(path: &str, request: &Request) -> Result<Response, Status> {
    let mut try_path = path.to_string();

    if !try_path.ends_with(".hbs") {
        // Try exact match
        match try_load_file(&try_path, &request) {
            Ok(response) => return Ok(response),
            Err(status) => match status {
                Status::NotFound => {}
                _ => {
                    return Err(status);
                }
            },
        }

        // Try next with .hbs suffix
        try_path.push_str(".hbs");
    }

    // Exact match template (handlebars)
    match try_load_file(&try_path, &request) {
        Ok(response) => {
            match render_response_body_for_request(
                path,
                &request.protocol().media_type(),
                request,
                &response,
            ) {
                Ok(rendered_response) => Ok(rendered_response),
                Err(status) => Err(status),
            }
        }
        Err(status) => Err(status),
    }
}

fn try_load_file(path: &str, request: &Request) -> Result<Response, Status> {
    let path_buf = match PathBuf::from(&path).canonicalize() {
        Ok(path) => path,
        Err(_) => return Err(Status::NotFound),
    };

    if !path_buf.starts_with(format!(
        "{}/",
        request.server_context().config().public_root_path()
    )) && !path_buf.starts_with(format!(
        "{}/",
        request.server_context().config().errdocs_path()
    )) {
        error!(
            "[{}] [{}] [{}] [{}] {}: canonicalized path not in public root/errdocs dir - path traversal attempt? (canonicalized path: {})",
            request.protocol(),
            request.peer_addr(),
            request.client_certificate_details(),
            request.path(),
            Status::OtherClientError,
            path
        );
        return Err(Status::OtherClientError);
    }

    if path_buf.is_file() {
        let resp_body = request.server_context().fs_read(path_buf);

        return match resp_body {
            Ok(body) => Ok(Response::new(
                Status::Success,
                mime_guess::from_path(&path)
                    .first_raw()
                    .unwrap_or(&request.protocol().media_type()),
                &body,
            )),
            Err(_) => Err(Status::Unauthorized),
        };
    }

    Err(Status::NotFound)
}
