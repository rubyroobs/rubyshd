use crate::request::Request;
use crate::response::{Response, Status};
use crate::templates::render_response_body_for_request;
use std::fs;
use std::path::PathBuf;

pub fn try_load_files_with_template(
    unsanitized_path: &String,
    request: &Request,
) -> Result<Response, Status> {
    let mut try_path = unsanitized_path.clone();

    if !try_path.ends_with(".hbs") {
        // Try exact match
        match try_load_file(&try_path, &request.protocol().mime_type()) {
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
    match try_load_file(&try_path, &request.protocol().mime_type()) {
        Ok(response) => {
            match render_response_body_for_request(unsanitized_path, request, &response) {
                Ok(rendered_response) => Ok(rendered_response),
                Err(status) => Err(status),
            }
        }
        Err(status) => Err(status),
    }
}

fn try_load_file(unsanitized_path: &str, default_mime_type: &str) -> Result<Response, Status> {
    let buf = PathBuf::from(unsanitized_path);
    if buf.is_file() {
        let resp_body: Result<Vec<u8>, std::io::Error> = fs::read(buf);

        return match resp_body {
            Ok(body) => Ok(Response::new(
                Status::Success,
                mime_guess::from_path(&unsanitized_path)
                    .first_raw()
                    .unwrap_or(default_mime_type),
                &body,
            )),
            Err(_) => Err(Status::Unauthorized),
        };
    }

    Err(Status::NotFound)
}
