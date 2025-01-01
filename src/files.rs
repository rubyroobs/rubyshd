use chrono::{DateTime, Utc};
use log::{error, info};
use serde_json::json;

use crate::request::Request;
use crate::response::{Response, Status};
use crate::templates::render_response_body_for_request;
use gray_matter::engine::YAML;
use gray_matter::Matter;
use std::path::PathBuf;

pub fn try_load_file_for_path(path: &str, request: &mut Request) -> Result<Response, Status> {
    let mut try_path = path.to_string();

    if !try_path.ends_with(".hbs") {
        // Try exact match
        match try_load_file(&try_path, request) {
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

    fn json_value_merge(dst: &mut serde_json::Value, src: serde_json::Value) {
        match (dst, src) {
            (dst @ &mut serde_json::Value::Object(_), serde_json::Value::Object(src)) => {
                let dst = dst.as_object_mut().unwrap();
                for (k, v) in src {
                    json_value_merge(dst.entry(k).or_insert(serde_json::Value::Null), v);
                }
            }
            (dst, src) => *dst = src,
        }
    }

    // Exact match template (handlebars)
    match try_load_file(&try_path, request) {
        Ok(response) => match String::from_utf8(response.body().to_vec()) {
            Ok(body) => {
                let matter = Matter::<YAML>::new();
                let result = matter.parse(&body);

                if let Some(front_matter) = result.data {
                    let front_matter_json: serde_json::Value = front_matter.into();
                    json_value_merge(&mut request.mut_template_context().meta, front_matter_json);
                }

                match render_response_body_for_request(
                    path,
                    request,
                    &Response::new(
                        *response.status(),
                        response.media_type(),
                        result.content.as_bytes(),
                        response.cacheable(),
                    ),
                ) {
                    Ok(rendered_response) => Ok(rendered_response),
                    Err(status) => Err(status),
                }
            }
            Err(err) => {
                error!(
                    "[{}] [{}] [{}] [{}] Unicode error reading {} (valid up to {})",
                    request.protocol(),
                    request.peer_addr(),
                    request.client_certificate_details(),
                    request.path(),
                    path,
                    err.utf8_error().valid_up_to()
                );
                Err(Status::OtherServerError)
            }
        },
        Err(status) => Err(status),
    }
}

fn try_load_file(path: &str, request: &mut Request) -> Result<Response, Status> {
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
        let resp_file = request.server_context().fs_read(path_buf);

        return match resp_file {
            Ok(file) => {
                if let (Ok(created), Ok(modified)) =
                    (file.metadata().created(), file.metadata().modified())
                {
                    if let Some(meta_obj) = request.mut_template_context().meta.as_object_mut() {
                        if !meta_obj.contains_key("created_at") {
                            let created_utc: DateTime<Utc> = created.clone().into();
                            meta_obj.insert("created_at".to_string(), json!(created_utc));
                        }

                        if !meta_obj.contains_key("updated_at") {
                            let modified_utc: DateTime<Utc> = modified.clone().into();
                            meta_obj.insert("updated_at".to_string(), json!(modified_utc));
                        }
                    }
                }

                Ok(Response::new(
                    Status::Success,
                    mime_guess::from_path(&path)
                        .first_raw()
                        .unwrap_or(&request.protocol().media_type()),
                    &file.data(),
                    true,
                ))
            }
            Err(_) => Err(Status::Unauthorized),
        };
    }

    Err(Status::NotFound)
}
