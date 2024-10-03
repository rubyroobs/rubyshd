use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use sanitize_filename::sanitize;
use log::{error, info};
use handlebars::Handlebars;
use x509_parser::nom::AsBytes;

use crate::request::Request;
use crate::response::{Response, Status};
use crate::{PUBLIC_PATH, TLS_LISTEN_PORT};

#[derive(serde::Serialize, serde::Deserialize)]
struct TemplateRequestData {
  peer_addr: SocketAddr,
  path: String,
  common_name: String,
}

pub fn route_request(request: &Request) -> Response {
  let sanitized_path = sanitize(request.path());

  // todo: check if path is directory, handle all that~

  match try_load_files_with_template(sanitized_path, request) {
    Ok(data) => match data {
      Some(body) => {
        info!(
          "OK [{} -> {}] [{}] [{}] {}",
          request.peer_addr(),
          TLS_LISTEN_PORT,
          request.protocol(),
          request.client_certificate_details(),
          request.path()
        );
        // TODO: return an Option of a struct or something with mime type and body... actually maybe just a response lol
        return Response::new(Status::Success, "text/plain", body.as_bytes())
      },
      None => {},
    },
    Err(error_str) => {
      // TODO: swap out error_str below with some sort of enum and then use that to return server errror for template errors or unauthorized for file perm errors
      error!(
        "Unauthorized [{} -> {}] [{}] [{}] {}",
        request.peer_addr(),
        TLS_LISTEN_PORT,
        request.protocol(),
        request.client_certificate_details(),
        request.path()
      );
      return Response::new(Status::Unauthorized, "text/plain", error_str.as_bytes()) 
    }
  }
  
  error!(
    "Not Found [{} -> {}] [{}] [{}] {}",
    request.peer_addr(),
    TLS_LISTEN_PORT,
    request.protocol(),
    request.client_certificate_details(),
    request.path()
  );
  Response::new(Status::NotFound, "text/plain", "Not Found".as_bytes())
}

fn try_load_files_with_template(path: String, request: &Request) -> Result<Option<Vec<u8>>, String> {
  // Exact match
  match try_load_file(&path) {
    Ok(file) => match file {
      Some(body) => {
        return Ok(Some(body))
      },
      None => {},
    },
    Err(error_str) => {
      return Err(error_str)
    }
  }

  // Exact match template (handlebars)
  match try_load_file(format!("{}.hbs", &path).as_str()) {
    Ok(file) => match file {
      Some(body) => {
        let handlebars: Handlebars<'_> = Handlebars::new();

        let request_data = TemplateRequestData{
          peer_addr: *request.peer_addr(),
          path: (*request.url().path()).to_string(),
          common_name: request.client_certificate_details().common_name()
        };

        match String::from_utf8(body) {
          Ok(template_body) => {
            match handlebars.render_template(&template_body, &request_data) {
              Ok(rendered_body) => {
                return Ok(Some(Vec::from(rendered_body.as_bytes())))
              },
              Err(_) => {
                return Err("Template error".to_string())
              }
            }
          },
          Err(_) => {
            return Err("Unicode error".to_string())
          }
        }
      },
      None => {},
    },
    Err(error_str) => {
      return Err(error_str)
    }
  }

  Ok(None)
}

fn try_load_file(path: &str) -> Result<Option<Vec<u8>>, String> {
  let buf = PathBuf::from(format!("{}/{}", PUBLIC_PATH, path));
  if buf.is_file() {
    let resp_body: Result<Vec<u8>, std::io::Error> = fs::read(buf);

    return match resp_body {
      Ok(body) => Ok(Some(body)),
      Err(_) => Err("Forbidden".to_string())
    }
  }
  
  Ok(None)
}