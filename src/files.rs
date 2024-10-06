use crate::request::Request;
use crate::response::Status;
use log::error;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use handlebars::Handlebars;

#[derive(serde::Serialize, serde::Deserialize)]
struct TemplateRequestData {
  peer_addr: SocketAddr,
  path: String,
  common_name: String,
}


pub fn try_load_files_with_template(unsanitized_path: &String, request: &Request) -> Result<Vec<u8>, Status> {
  let mut try_path = unsanitized_path.clone();

  if !try_path.ends_with(".hbs") {
    // Try exact match
    match try_load_file(&try_path) {
      Ok(body) => {
        return Ok(body)
      },
      Err(status) => match status {
        Status::NotFound => {},
        _ => {
          return Err(status);
        }
      }
    }

    // Try next with .hbs suffix
    try_path.push_str(".hbs");
  }

  // Exact match template (handlebars)
  match try_load_file(&try_path) {
    Ok(body) => {
      let handlebars: Handlebars<'_> = Handlebars::new();

      let request_data = TemplateRequestData{
        peer_addr: *request.peer_addr(),
        path: (*request.url().path()).to_string(),
        common_name: request.client_certificate_details().common_name()
      };

      match String::from_utf8(body) {
        Ok(template_body) => {
          match handlebars.render_template(&template_body, &request_data) {
            Ok(rendered_body) => Ok(Vec::from(rendered_body.as_bytes())),
            Err(err) => {
              error!(
                "[{}] [{}] [{}] [{}] Handlebars error in {}: {}",
                request.protocol(),
                request.peer_addr(),
                request.client_certificate_details(),
                request.path(),
                try_path,
                err
              );
              Err(Status::OtherServerError)
            }
          }
        },
        Err(err) => {
          error!(
            "[{}] [{}] [{}] [{}] Unicode error reading {} (valid up to {})",
            request.protocol(),
            request.peer_addr(),
            request.client_certificate_details(),
            request.path(),
            try_path,
            err.utf8_error().valid_up_to()
          );
          Err(Status::OtherServerError)
        }
      }
    },
    Err(status) => Err(status)
  }
}

fn try_load_file(unsanitized_path: &str) -> Result<Vec<u8>, Status> {
  let buf = PathBuf::from(unsanitized_path);
  if buf.is_file() {
    let resp_body: Result<Vec<u8>, std::io::Error> = fs::read(buf);

    return match resp_body {
      Ok(body) => Ok(body),
      Err(_) => Err(Status::Unauthorized),
    }
  }
  
  Err(Status::NotFound)
}

