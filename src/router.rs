use std::path::PathBuf;

use log::{error, info};
use sanitize_filename::sanitize;

use crate::files::try_load_files_with_template;
use crate::request::Request;
use crate::response::{Response, Status};

pub fn route_request(request: &Request) -> Response {
    let sanitized_path = sanitize(request.path());
    let root_path = format!(
        "{}/{}",
        request.server_config().public_root_path(),
        sanitized_path
    );
    let is_directory = PathBuf::from(&root_path).is_dir();

    if is_directory {
        match try_route_request_for_path(&format!("{}/index.hbs", root_path), request) {
            Some(response) => {
                return response;
            }
            None => {}
        }

        for try_ext in request.protocol().media_type_file_extensions() {
            let try_path = if root_path.ends_with("/") {
                format!("{}/index.{}", root_path, try_ext)
            } else {
                format!("{}/index.{}", root_path, try_ext)
            };

            match try_route_request_for_path(&try_path, request) {
                Some(response) => {
                    return response;
                }
                None => {}
            }
        }
    } else {
        match try_route_request_for_path(&root_path, request) {
            Some(response) => {
                return response;
            }
            None => {}
        }
    }

    // whelp, we tried our best :c
    // TODO: directory listing if is_directory?
    error!(
        "[{}] [{}] [{}] [{}] {}",
        request.protocol(),
        request.peer_addr(),
        request.client_certificate_details(),
        request.path(),
        Status::NotFound,
    );
    return Response::new_for_request_and_status(request, Status::NotFound);
}

// Tries to load a file, if it exists it will return a response with the contents or the error loading/rendering them
fn try_route_request_for_path(try_path: &String, request: &Request) -> Option<Response> {
    match try_load_files_with_template(&try_path, request) {
        Ok(response) => {
            info!(
                "[{}] [{}] [{}] [{}] {} (from file: {})",
                request.protocol(),
                request.peer_addr(),
                request.client_certificate_details(),
                request.path(),
                response.status(),
                try_path,
            );
            Some(response)
        }
        Err(status) => match status {
            Status::NotFound => None,
            _ => {
                error!(
                    "[{}] [{}] [{}] [{}] {} (from file: {})",
                    request.protocol(),
                    request.peer_addr(),
                    request.client_certificate_details(),
                    request.path(),
                    status,
                    try_path,
                );
                Some(Response::new_for_request_and_status(request, status))
            }
        },
    }
}
