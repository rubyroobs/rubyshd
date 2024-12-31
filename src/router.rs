use std::path::PathBuf;

use log::{error, info};

use crate::files::try_load_file_for_path;
use crate::protocol::Protocol;
use crate::request::Request;
use crate::response::{Response, Status};
use crate::templates::{render_markdown_response_for_request, Markup};

pub fn route_request(request: &mut Request) -> Response {
    let os_path_str = format!(
        "{}{}",
        request.server_context().config().public_root_path(),
        request.path()
    );
    let path_buf = PathBuf::from(&os_path_str);

    let is_directory = path_buf.is_dir();
    let trailing_slash = os_path_str.ends_with("/");

    // Generate a path stripped of known protocol-markup associated extensions + markdown/md
    let mut ext_stripped_os_path_str = os_path_str
        .strip_suffix(".md")
        .unwrap_or(&os_path_str)
        .to_string();

    if os_path_str.ends_with(".md") {
        request.mut_template_context().markup = Markup::Markdown
    }

    for try_ext in Protocol::Gemini.media_type_file_extensions() {
        let try_file_ext = &format!(".{}", try_ext);

        if os_path_str.ends_with(try_file_ext) {
            request.mut_template_context().markup = Markup::default_for_protocol(Protocol::Gemini)
        }

        ext_stripped_os_path_str = ext_stripped_os_path_str
            .strip_suffix(try_file_ext)
            .unwrap_or(&ext_stripped_os_path_str)
            .to_string()
    }

    for try_ext in Protocol::Https.media_type_file_extensions() {
        let try_file_ext = &format!(".{}", try_ext);

        if os_path_str.ends_with(try_file_ext) {
            request.mut_template_context().markup = Markup::default_for_protocol(Protocol::Https)
        }

        ext_stripped_os_path_str = ext_stripped_os_path_str
            .strip_suffix(try_file_ext)
            .unwrap_or(&ext_stripped_os_path_str)
            .to_string()
    }

    if is_directory {
        // explicit logic for directory indexes
        let try_path = match trailing_slash {
            true => format!("{}index.hbs", os_path_str),
            false => format!("{}/index.hbs", os_path_str),
        };

        match try_route_request_for_path(&try_path, request) {
            Some(response) => {
                return response;
            }
            None => {}
        }

        for try_ext in request.protocol().media_type_file_extensions() {
            let try_path = match trailing_slash {
                true => format!("{}index.{}", os_path_str, try_ext),
                false => format!("{}/index.{}", os_path_str, try_ext),
            };

            match try_route_request_for_path(&try_path, request) {
                Some(response) => {
                    return response;
                }
                None => {}
            }
        }
    } else {
        // First try exact requested path UNLESS .md file extension which gets handled later
        if !os_path_str.ends_with(".md") {
            match try_route_request_for_path(&os_path_str, request) {
                Some(response) => {
                    return response;
                }
                None => {}
            }
        }

        // Next see if the protocol appropriate default is available
        // TODO: use Accept here for HTTP which would be more appropriate
        for try_ext in request.protocol().media_type_file_extensions() {
            match try_route_request_for_path(&format!("{}.{}", os_path_str, try_ext), request) {
                Some(response) => {
                    return response;
                }
                None => {}
            }
        }

        // Markdown
        let try_path = format!("{}.md", ext_stripped_os_path_str);
        match try_route_request_for_path(&try_path, request) {
            Some(response) => {
                match render_markdown_response_for_request(request, &response, &try_path) {
                    Ok(rendered_response) => {
                        return rendered_response;
                    }
                    Err(status) => {
                        error!(
                            "[{}] [{}] [{}] [{}] {} (from file: {})",
                            request.protocol(),
                            request.peer_addr(),
                            request.client_certificate_details(),
                            request.path(),
                            status,
                            try_path,
                        );
                        Some(Response::new_for_request_and_status(request, status));
                    }
                }
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
fn try_route_request_for_path(try_path: &str, request: &mut Request) -> Option<Response> {
    match try_load_file_for_path(try_path, request) {
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
