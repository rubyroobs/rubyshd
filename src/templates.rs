use handlebars::{
    to_json, Context, Decorator, Handlebars, Helper, HelperDef, HelperResult, JsonRender, Output,
    RenderContext, RenderError, RenderErrorReason, ScopedJson,
};
use handlebars_chrono::HandlebarsChronoDateTime;
use log::error;
use rand::seq::{IteratorRandom as _, SliceRandom};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use std::fmt;
use std::net::SocketAddr;
use std::str::FromStr;

use crate::context::PageMetadata;
use crate::md2gemtext;
use crate::protocol::Protocol;
use crate::request::Request;
use crate::response::{Response, Status};

pub const DEFAULT_BLANK_PARTIAL_NAME: &str = "blank";

#[derive(Copy, Clone, Debug, PartialEq, Eq, SerializeDisplay, DeserializeFromStr)]
pub enum Markup {
    Html,
    Gemtext,
    Markdown,
}

impl fmt::Display for Markup {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Markup::Gemtext => write!(f, "Gemtext"),
            Markup::Html => write!(f, "HTML"),
            Markup::Markdown => write!(f, "Markdown"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseMarkupError;

impl fmt::Display for ParseMarkupError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ParseMarkupError")
    }
}

impl FromStr for Markup {
    type Err = ParseMarkupError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Gemtext" => Ok(Markup::Gemtext),
            "HTML" => Ok(Markup::Html),
            "Markdown" => Ok(Markup::Markdown),
            _ => Err(ParseMarkupError),
        }
    }
}

impl Markup {
    pub fn default_for_protocol(protocol: Protocol) -> Markup {
        match protocol {
            Protocol::Gemini => Markup::Gemtext,
            Protocol::Https => Markup::Html,
        }
    }

    pub fn media_type(&self) -> String {
        match self {
            Markup::Gemtext => Protocol::Gemini.media_type(),
            Markup::Html => Protocol::Https.media_type(),
            Markup::Markdown => "text/markdown; charset=utf-8".into(),
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TemplateRequestContext {
    pub meta: serde_json::Value,
    pub data: serde_json::Value,
    pub posts: Vec<PageMetadata>,
    pub peer_addr: SocketAddr,
    pub path: String,
    pub is_authenticated: bool,
    pub is_anonymous: bool,
    pub common_name: String,
    pub protocol: Protocol,
    pub markup: Markup,
    pub is_gemini: bool,
    pub is_https: bool,
    pub os_platform: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TemplateResponseContext {
    status: Option<String>,
    media_type: Option<String>,
    redirect_uri: Option<String>,
    redirect_permanent: Option<bool>,
}

pub fn initialize_handlebars(handlebars: &mut Handlebars) {
    handlebars.register_helper("datetime", Box::new(HandlebarsChronoDateTime));
    handlebars.register_helper(
        "private-context-serialize",
        Box::new(serialize_context_helper),
    );
    handlebars.register_helper("pick-random", Box::new(pick_random_helper));
    handlebars.register_helper("partial-for-markup", Box::new(partial_for_markup_helper));
    handlebars.register_decorator("temporary-redirect", Box::new(temporary_redirect_decorator));
    handlebars.register_decorator("permanent-redirect", Box::new(permanent_redirect_decorator));
    handlebars.register_decorator("status", Box::new(status_decorator));
    handlebars.register_decorator("media-type", Box::new(media_type_decorator));
}

pub fn render_response_body_for_request(
    loaded_path: &str,
    request: &Request,
    response: &Response,
) -> Result<Response, Status> {
    let body = response.body().to_vec();

    match String::from_utf8(body) {
        Ok(template_body) => match render_template(request, &template_body) {
            Ok((rendered_body, response_context)) => {
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
                    None => request.template_context().markup.media_type().to_owned(),
                };

                match response_context.redirect_uri {
                    None => Ok(Response::new(
                        status,
                        &media_type,
                        rendered_body.as_bytes(),
                        false,
                    )),
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
        },
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

fn render_template(
    request: &Request,
    template_string: &str,
) -> Result<(String, TemplateResponseContext), handlebars::RenderError> {
    let mut template_string = template_string.to_string();

    template_string.push_str("\n{{private-context-serialize}}");

    match request
        .server_context()
        .handlebars_render_template(&template_string, &request.template_context())
    {
        Ok(raw_rendered_body) => {
            let (rendered_body, resp_context_str) = raw_rendered_body
                .rsplit_once("\n")
                .unwrap_or((&raw_rendered_body, "{}"));

            let response_context: TemplateResponseContext = serde_json::from_str(resp_context_str)
                .unwrap_or(TemplateResponseContext {
                    status: None,
                    media_type: None,
                    redirect_uri: None,
                    redirect_permanent: None,
                });
            Ok((rendered_body.to_string(), response_context))
        }
        Err(err) => Err(err),
    }
}

pub fn render_markdown_response_for_request(
    request: &Request,
    response: &Response,
    loaded_path: &str,
) -> Result<Response, Status> {
    match String::from_utf8(response.body().to_vec()) {
        Ok(resp_body_str) => {
            // Remove <?POSTPROCESS ... POSTPROCESS?> processing tags used to keep post-processable handlebars calls from being encoded
            let strip_postprocess_tags = |str: String| -> String {
                str.replace("<?POSTPROCESS ", "")
                    .replace("<?POSTPROCESS", "")
                    .replace(" POSTPROCESS?>", "")
                    .replace("POSTPROCESS?>", "")
            };

            let rendered_md = match request.template_context().markup {
                Markup::Gemtext => strip_postprocess_tags(md2gemtext::convert(&resp_body_str)),
                Markup::Html => match markdown::to_html_with_options(
                    &resp_body_str,
                    &markdown::Options {
                        compile: markdown::CompileOptions {
                            allow_dangerous_html: true,
                            ..markdown::CompileOptions::default()
                        },
                        ..markdown::Options::default()
                    },
                ) {
                    Ok(str) => {
                        // Strip AFTER for markdown::to_html_with_options as otherwise handlebars get turned into HTML entities
                        strip_postprocess_tags(str)
                    }
                    Err(err) => {
                        error!("Error converting markdown to HTML: {}", err);
                        return Err(Status::OtherServerError);
                    }
                },
                Markup::Markdown => strip_postprocess_tags(resp_body_str), // Markdown just needs the meta tags stripping...
            };

            let md_response = Response::new(
                *response.status(),
                &request.template_context().protocol.media_type(),
                rendered_md.as_bytes(),
                false,
            );

            match render_response_body_for_request(loaded_path, request, &md_response) {
                Ok(rerendered_md_resp) => Ok(rerendered_md_resp),
                Err(status) => Err(status),
            }
        }
        Err(err) => {
            error!(
                "Unicode error in pre-rendered markdown template (valid up to {})",
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

#[allow(non_camel_case_types)]
pub struct pick_random_helper;

impl HelperDef for pick_random_helper {
    fn call_inner<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'rc>,
        _: &'reg Handlebars,
        _: &'rc Context,
        _: &mut RenderContext<'reg, 'rc>,
    ) -> Result<ScopedJson<'reg>, RenderError> {
        let param = h
            .param(0)
            .ok_or(RenderErrorReason::ParamNotFoundForIndex("pick-random", 0))?;

        if let Some(array) = param.value().as_array() {
            match array.choose(&mut rand::thread_rng()) {
                Some(value) => Ok(ScopedJson::Derived(value.clone())),
                None => Ok(ScopedJson::Derived(serde_json::Value::Null)),
            }
        } else if let Some(object) = param.value().as_object() {
            match object.values().choose(&mut rand::thread_rng()) {
                Some(value) => Ok(ScopedJson::Derived(value.clone())),
                None => Ok(ScopedJson::Derived(serde_json::Value::Null)),
            }
        } else {
            // TODO: raise an invalid param error here?
            Ok(ScopedJson::Derived(serde_json::Value::Null))
        }
    }
}

#[allow(non_camel_case_types)]
pub struct partial_for_markup_helper;

impl HelperDef for partial_for_markup_helper {
    fn call_inner<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'rc>,
        hb: &'reg Handlebars,
        rc: &'rc Context,
        _: &mut RenderContext<'reg, 'rc>,
    ) -> Result<ScopedJson<'reg>, RenderError> {
        let param = h.param(0).ok_or(RenderErrorReason::ParamNotFoundForIndex(
            "partial-for-markup",
            0,
        ))?;

        match rc
            .data()
            .as_object()
            .unwrap()
            .get("markup")
            .unwrap()
            .as_str()
            .unwrap()
        {
            "Gemtext" => Ok(ScopedJson::Derived(serde_json::Value::String(format!(
                "{}.gmi",
                param.value().render()
            )))),
            "HTML" => Ok(ScopedJson::Derived(serde_json::Value::String(format!(
                "{}.html",
                param.value().render()
            )))),
            _ => Ok(ScopedJson::Derived(serde_json::Value::String(
                DEFAULT_BLANK_PARTIAL_NAME.to_string(),
            ))),
        }
    }
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
    let mut new_ctx = match rc.context() {
        Some(rc_ctx) => rc_ctx.as_ref().clone(),
        None => ctx.clone(),
    };

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

    let mut new_ctx = match rc.context() {
        Some(rc_ctx) => rc_ctx.as_ref().clone(),
        None => ctx.clone(),
    };

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
    let mut new_ctx = match rc.context() {
        Some(rc_ctx) => rc_ctx.as_ref().clone(),
        None => ctx.clone(),
    };
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
    let mut new_ctx = match rc.context() {
        Some(rc_ctx) => rc_ctx.as_ref().clone(),
        None => ctx.clone(),
    };

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
