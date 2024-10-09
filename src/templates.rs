use log::error;
use std::net::SocketAddr;
use std::str::FromStr;

use handlebars::{
    to_json, Context, Decorator, Handlebars, Helper, HelperResult, JsonRender, Output,
    RenderContext, RenderError, RenderErrorReason,
};

use crate::protocol::Protocol;
use crate::request::Request;
use crate::response::{Response, Status};

#[derive(serde::Serialize, serde::Deserialize)]
struct TemplateRequestContext {
    data: serde_json::Value,
    peer_addr: SocketAddr,
    path: String,
    is_authenticated: bool,
    is_anonymous: bool,
    common_name: String,
    protocol: String,
    is_gemini: bool,
    is_https: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TemplateResponseContext {
    status: Option<String>,
    media_type: Option<String>,
    redirect_uri: Option<String>,
    redirect_permanent: Option<bool>,
}

pub fn initialize_handlebars(handlebars: &mut Handlebars) {
    handlebars.register_helper(
        "private-context-serialize",
        Box::new(serialize_context_helper),
    );
    handlebars.register_decorator("temporary-redirect", Box::new(temporary_redirect_decorator));
    handlebars.register_decorator("permanent-redirect", Box::new(permanent_redirect_decorator));
    handlebars.register_decorator("status", Box::new(status_decorator));
    handlebars.register_decorator("media-type", Box::new(media_type_decorator));
}

pub fn render_response_body_for_request(
    loaded_path: &str,
    default_media_type: &str,
    request: &Request,
    response: &Response,
) -> Result<Response, Status> {
    let body = response.body().to_vec();

    let request_data = TemplateRequestContext {
        data: request.server_context().get_data(),
        peer_addr: *request.peer_addr(),
        path: (*request.url().path()).to_string(),
        is_authenticated: !request.client_certificate_details().is_anonymous(),
        is_anonymous: request.client_certificate_details().is_anonymous(),
        common_name: request.client_certificate_details().common_name(),
        protocol: request.protocol().to_string(),
        is_gemini: request.protocol() == Protocol::Gemini,
        is_https: request.protocol() == Protocol::Https,
    };

    match String::from_utf8(body) {
        Ok(mut template_body) => {
            template_body.push_str("\n{{private-context-serialize}}");
            match request
                .server_context()
                .handlebars_render_template(&template_body, &request_data)
            {
                Ok(raw_rendered_body) => {
                    let (rendered_body, resp_context_str) = raw_rendered_body
                        .rsplit_once("\n")
                        .unwrap_or((&raw_rendered_body, "{}"));

                    let response_context: TemplateResponseContext =
                        serde_json::from_str(resp_context_str).unwrap_or(TemplateResponseContext {
                            status: None,
                            media_type: None,
                            redirect_uri: None,
                            redirect_permanent: None,
                        });

                    let status = match response_context.status {
                        Some(status_str) => match Status::from_str(&status_str) {
                            Ok(status) => status,
                            Err(_) => {
                                error!(
                                  "[{}] [{}] [{}] [{}] Handlebars error in {}: status set to unknown status code {}",
                                  request.protocol(),
                                  request.peer_addr(),
                                  request.client_certificate_details(),
                                  request.path(),
                                  loaded_path,
                                  status_str
                                );
                                Status::Success
                            }
                        },
                        None => match response_context.redirect_permanent {
                            Some(is_permanent) => match is_permanent {
                                true => Status::PermanentRedirect,
                                false => Status::TemporaryRedirect,
                            },
                            None => Status::Success,
                        },
                    };

                    let media_type = match response_context.media_type {
                        Some(context_media_type) => context_media_type.to_owned(),
                        None => default_media_type.to_owned(),
                    };

                    match response_context.redirect_uri {
                        None => Ok(Response::new(status, &media_type, rendered_body.as_bytes())),
                        Some(redirect_uri) => {
                            Ok(Response::new_with_redirect_uri(status, &redirect_uri))
                        }
                    }
                }
                Err(err) => {
                    error!(
                        "[{}] [{}] [{}] [{}] Handlebars error in {}: {}",
                        request.protocol(),
                        request.peer_addr(),
                        request.client_certificate_details(),
                        request.path(),
                        loaded_path,
                        err
                    );
                    Err(Status::OtherServerError)
                }
            }
        }
        Err(err) => {
            error!(
                "[{}] [{}] [{}] [{}] Unicode error reading {} (valid up to {})",
                request.protocol(),
                request.peer_addr(),
                request.client_certificate_details(),
                request.path(),
                loaded_path,
                err.utf8_error().valid_up_to()
            );
            Err(Status::OtherServerError)
        }
    }
}

fn serialize_context_helper(
    _: &Helper,
    _: &Handlebars,
    _: &Context,
    rc: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    match &rc.context() {
        Some(context) => out.write(&context.data().to_string())?,
        None => {}
    }
    Ok(())
}

fn status_decorator<'reg: 'rc, 'rc>(
    d: &Decorator,
    _: &Handlebars,
    ctx: &Context,
    rc: &mut RenderContext,
) -> Result<(), RenderError> {
    let param = d
        .param(0)
        .ok_or(RenderErrorReason::ParamNotFoundForIndex("status", 0))?;
    let mut new_ctx = ctx.clone();
    {
        let data = new_ctx.data_mut();
        if let Some(ref mut m) = data.as_object_mut() {
            m.insert("status".to_string(), to_json(param.value().render()));
        }
    }
    rc.set_context(new_ctx);
    Ok(())
}

fn media_type_decorator<'reg: 'rc, 'rc>(
    d: &Decorator,
    _: &Handlebars,
    ctx: &Context,
    rc: &mut RenderContext,
) -> Result<(), RenderError> {
    let param = d
        .param(0)
        .ok_or(RenderErrorReason::ParamNotFoundForIndex("media-type", 0))?;
    let mut new_ctx = ctx.clone();
    {
        let data = new_ctx.data_mut();
        if let Some(ref mut m) = data.as_object_mut() {
            m.insert("media_type".to_string(), to_json(param.value().render()));
        }
    }
    rc.set_context(new_ctx);
    Ok(())
}

fn temporary_redirect_decorator<'reg: 'rc, 'rc>(
    d: &Decorator,
    _: &Handlebars,
    ctx: &Context,
    rc: &mut RenderContext,
) -> Result<(), RenderError> {
    let param = d.param(0).ok_or(RenderErrorReason::ParamNotFoundForIndex(
        "temporary-redirect",
        0,
    ))?;
    let mut new_ctx = ctx.clone();
    {
        let data = new_ctx.data_mut();
        if let Some(ref mut m) = data.as_object_mut() {
            m.insert("redirect_permanent".to_string(), to_json(false));
            m.insert("redirect_uri".to_string(), to_json(param.value().render()));
        }
    }
    rc.set_context(new_ctx);
    Ok(())
}

fn permanent_redirect_decorator<'reg: 'rc, 'rc>(
    d: &Decorator,
    _: &Handlebars,
    ctx: &Context,
    rc: &mut RenderContext,
) -> Result<(), RenderError> {
    let param = d.param(0).ok_or(RenderErrorReason::ParamNotFoundForIndex(
        "permanent-redirect",
        0,
    ))?;
    let mut new_ctx = ctx.clone();
    {
        let data = new_ctx.data_mut();
        if let Some(ref mut m) = data.as_object_mut() {
            m.insert("redirect_permanent".to_string(), to_json(true));
            m.insert("redirect_uri".to_string(), to_json(param.value().render()));
        }
    }
    rc.set_context(new_ctx);
    Ok(())
}
