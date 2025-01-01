use std::{
    ffi::{OsStr, OsString},
    fs::{self, Metadata},
    path::PathBuf,
    sync::Mutex,
    time::SystemTime,
};

use crate::{
    config::Config,
    protocol::Protocol,
    templates::{initialize_handlebars, DEFAULT_BLANK_PARTIAL_NAME},
};
use cached::stores::ExpiringSizedCache;
use chrono::{DateTime, Utc};
use gray_matter::{engine::YAML, Matter, Pod};
use handlebars::Handlebars;
use log::{debug, error};
use serde::Serialize;
use serde_json::json;
use walkdir::WalkDir;

const MAX_FS_CACHE_ENTRIES: usize = 512;
const MAX_FS_CACHE_LONG_TTL_MS: u64 = 14_400_000;

const MAX_FS_CACHE_SHORT_TTL_EXTENSIONS: &[&str] = &["hbs", "html", "gmi", "md", "json"];
const MAX_FS_CACHE_SHORT_TTL_MS: u64 = 10_000;

const MAX_DATA_CACHE_ENTRIES: usize = 512;
const MAX_DATA_CACHE_TTL_MS: u64 = 10_000;

#[derive(Debug, Clone)]
pub struct PageMetadata {
    path: String,
    protocol: Protocol,
    title: String,
    description: Option<String>,
    date: DateTime<Utc>,
    is_post: bool,
}

#[derive(Debug, Clone)]
pub struct CachedFile {
    data: Vec<u8>,
    metadata: Metadata,
}

impl CachedFile {
    pub fn data(&self) -> &Vec<u8> {
        &self.data
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
}

pub struct ServerContext {
    config: Config,
    handlebars: Mutex<Handlebars<'static>>,
    fs_cache: Mutex<ExpiringSizedCache<OsString, CachedFile>>,
    data_cache: Mutex<ExpiringSizedCache<OsString, serde_json::Value>>,
}

#[derive(Debug)]
pub enum DataReadErr {
    JsonError(serde_json::Error),
    Utf8Error(std::str::Utf8Error),
    IoError(std::io::Error),
}

impl ServerContext {
    pub fn new_with_config(config: Config) -> ServerContext {
        let mut handlebars = Handlebars::new();
        initialize_handlebars(&mut handlebars);

        ServerContext {
            config: config,
            handlebars: Mutex::new(handlebars),
            fs_cache: Mutex::new(ExpiringSizedCache::with_capacity(
                MAX_FS_CACHE_LONG_TTL_MS,
                MAX_FS_CACHE_ENTRIES,
            )),
            data_cache: Mutex::new(ExpiringSizedCache::with_capacity(
                MAX_DATA_CACHE_TTL_MS,
                MAX_DATA_CACHE_ENTRIES,
            )),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn handlebars_render_template<T>(
        &self,
        template_string: &str,
        data: T,
    ) -> Result<std::string::String, handlebars::RenderError>
    where
        T: Serialize,
    {
        self.register_handlebars_templates();
        self.handlebars
            .lock()
            .unwrap()
            .render_template(template_string, &data)
    }

    fn register_handlebars_templates(&self) {
        for entry in WalkDir::new(self.config().partials_path())
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path_buf = entry.into_path();
            let path_str = path_buf.to_str().unwrap();
            if path_str.ends_with(".hbs") {
                let partial_name = path_str
                    .strip_prefix(&format!("{}/", self.config().partials_path()))
                    .unwrap()
                    .strip_suffix(".hbs")
                    .unwrap()
                    .to_string();

                match self.fs_read(path_buf) {
                    Ok(file) => match std::str::from_utf8(&file.data()) {
                        Ok(value) => {
                            let mut handlebars = self.handlebars.lock().unwrap();
                            match handlebars.register_template_string(&partial_name, value) {
                                Ok(_) => {}
                                Err(err) => error!(
                                    "ERROR registering handlebar partial {}: {}",
                                    partial_name, err
                                ),
                            }
                        }
                        Err(err) => error!(
                            "ERROR loading handlebar partial {} as UTF-8: {}",
                            partial_name, err
                        ),
                    },
                    Err(err) => {
                        error!(
                            "ERROR loading handlebar partial file {}: {}",
                            partial_name, err
                        )
                    }
                }
            }
        }

        // Register special "blank" partial
        let mut handlebars = self.handlebars.lock().unwrap();
        match handlebars.register_template_string(DEFAULT_BLANK_PARTIAL_NAME, "") {
            Ok(_) => {}
            Err(err) => error!("ERROR registering default handlebar partial blank: {}", err),
        }
    }

    pub fn fs_read(&self, path_buf: PathBuf) -> Result<CachedFile, std::io::Error> {
        let cloned_path_buf = path_buf.clone();
        let cache_key = cloned_path_buf.as_os_str().to_os_string();
        let mut fs_cache = self.fs_cache.lock().unwrap();

        match fs_cache.get(&cache_key) {
            Some(file) => {
                debug!("fs cache hit: {:?}", cache_key);
                Ok(file.clone())
            }
            None => match (fs::read(path_buf.clone()), fs::metadata(path_buf.clone())) {
                (Ok(data), Ok(metadata)) => {
                    let cached_file = CachedFile {
                        data: data.clone(),
                        metadata: metadata.clone(),
                    };
                    if MAX_FS_CACHE_SHORT_TTL_EXTENSIONS.contains(
                        &cloned_path_buf
                            .extension()
                            .unwrap_or(OsStr::new(""))
                            .to_str()
                            .unwrap_or(""),
                    ) {
                        debug!("fs cache miss (short ttl): {:?}", cache_key);
                        match fs_cache.insert_ttl(
                            cache_key.clone(),
                            cached_file.clone(),
                            MAX_FS_CACHE_SHORT_TTL_MS,
                        ) {
                            Ok(_) => {}
                            Err(err) => error!(
                                "ERROR short-ttl fs cache insert for {:?}: {:?}",
                                cache_key, err
                            ),
                        }
                    } else {
                        debug!("fs cache miss (long ttl): {:?}", cache_key);
                        match fs_cache.insert(cache_key.clone(), cached_file.clone()) {
                            Ok(_) => {}
                            Err(err) => error!(
                                "ERROR long-ttl fs cache insert for {:?}: {:?}",
                                cache_key, err
                            ),
                        }
                    }
                    Ok(cached_file)
                }
                (Err(err), _) => Err(err),
                (_, Err(err)) => Err(err),
            },
        }
    }

    pub fn get_page_metadata(&self) -> Vec<PageMetadata> {
        WalkDir::new(self.config().public_root_path())
            .follow_links(false)
            .into_iter()
            .flat_map(|e| match e {
                Ok(entry) => {
                    let path_buf = entry.into_path();
                    match path_buf.clone().to_str() {
                        Some(path_str) if path_str.ends_with(".hbs") => {
                            match self.fs_read(path_buf) {
                                Ok(file) => match std::str::from_utf8(&file.data()) {
                                    Ok(data_str) => {
                                        let matter = Matter::<YAML>::new();
                                        if let Ok(data) = matter
                                            .parse(data_str)
                                            .data
                                            .unwrap_or(Pod::Null)
                                            .as_hashmap()
                                        {
                                            if !data
                                                .get("unlisted")
                                                .unwrap_or(&Pod::Null)
                                                .as_bool()
                                                .unwrap_or(false)
                                            {
                                                let title = data
                                                    .get("title")
                                                    .unwrap_or(&Pod::Null)
                                                    .as_string()
                                                    .unwrap_or("Untitled page".to_string());

                                                let description = data
                                                    .get("description")
                                                    .unwrap_or(&Pod::Null)
                                                    .as_string()
                                                    .ok();

                                                let date = match data
                                                    .get("date")
                                                    .unwrap_or(&Pod::Null)
                                                    .as_string()
                                                    .ok()
                                                {
                                                    Some(date_str) => {
                                                        match DateTime::parse_from_rfc3339(
                                                            &date_str,
                                                        ) {
                                                            Ok(date) => {
                                                                Some(date.with_timezone(&Utc))
                                                            }
                                                            Err(_) => None,
                                                        }
                                                    }
                                                    None => None,
                                                }
                                                .unwrap_or(
                                                    file.metadata
                                                        .modified()
                                                        .unwrap_or(SystemTime::now())
                                                        .into(),
                                                );

                                                let is_post = data
                                                    .get("post")
                                                    .unwrap_or(&Pod::Null)
                                                    .as_bool()
                                                    .ok()
                                                    .unwrap_or(false);

                                                // todo better protocol handling here
                                                let (protocols, uri_path) = if let Some(uri_path) =
                                                    path_str.strip_suffix(".html.hbs")
                                                {
                                                    ([Protocol::Https].iter(), uri_path.to_string())
                                                } else if let Some(uri_path) =
                                                    path_str.strip_suffix(".gmi.hbs")
                                                {
                                                    (
                                                        [Protocol::Gemini].iter(),
                                                        uri_path.to_string(),
                                                    )
                                                } else if let Some(uri_path) =
                                                    path_str.strip_suffix(".md.hbs")
                                                {
                                                    (
                                                        [Protocol::Https, Protocol::Gemini].iter(),
                                                        uri_path.to_string(),
                                                    )
                                                } else {
                                                    (([].iter()), "".to_string())
                                                };

                                                let normalized_uri_path = if uri_path
                                                    .ends_with("/index")
                                                {
                                                    let base = uri_path
                                                        .strip_prefix(
                                                            self.config().public_root_path(),
                                                        )
                                                        .unwrap()
                                                        .to_string();
                                                    match base.strip_suffix("/index") {
                                                        Some(path) if path == "" => "/".to_string(),
                                                        Some(path) => path.to_string(),
                                                        None => base,
                                                    }
                                                } else {
                                                    uri_path
                                                        .strip_prefix(
                                                            self.config().public_root_path(),
                                                        )
                                                        .unwrap()
                                                        .to_string()
                                                };

                                                protocols
                                                    .map(|protocol| PageMetadata {
                                                        title: title.to_string(),
                                                        path: normalized_uri_path.to_string(),
                                                        protocol: *protocol,
                                                        description: match &description {
                                                            Some(description) => {
                                                                Some(description.to_string())
                                                            }
                                                            None => None,
                                                        },
                                                        date: date,
                                                        is_post: is_post,
                                                    })
                                                    .collect::<Vec<PageMetadata>>()
                                            } else {
                                                Vec::<PageMetadata>::new()
                                            }
                                        } else {
                                            Vec::<PageMetadata>::new()
                                        }
                                    }
                                    Err(_) => Vec::<PageMetadata>::new(),
                                },
                                Err(_) => Vec::<PageMetadata>::new(),
                            }
                        }
                        Some(_) => Vec::<PageMetadata>::new(),
                        None => Vec::<PageMetadata>::new(),
                    }
                }
                Err(_) => Vec::<PageMetadata>::new(),
            })
            .collect::<Vec<PageMetadata>>()
    }

    pub fn get_data(&self) -> serde_json::Value {
        let mut data = json!({});

        for entry in WalkDir::new(self.config().data_path())
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path_buf = entry.into_path();
            let path_str = path_buf.to_str().unwrap();
            if path_str.ends_with(".json") {
                let data_key = path_str
                    .strip_prefix(&format!("{}/", self.config().data_path()))
                    .unwrap()
                    .strip_suffix(".json")
                    .unwrap()
                    .to_string();

                match self.data_read(path_buf) {
                    Ok(value) => {
                        data.as_object_mut().unwrap().insert(data_key, value);
                    }
                    Err(err) => {
                        error!("ERROR reading data JSON file {}: {:?}", data_key, err)
                    }
                }
            }
        }

        data
    }

    fn data_read(&self, path_buf: PathBuf) -> Result<serde_json::Value, DataReadErr> {
        let cloned_path_buf = path_buf.clone();
        let cache_key = cloned_path_buf.as_os_str().to_os_string();
        let mut data_cache = self.data_cache.lock().unwrap();

        match data_cache.get(&cache_key) {
            Some(data) => {
                debug!("data cache hit: {:?}", cache_key);
                Ok(data.clone())
            }
            None => match fs::read(path_buf) {
                Ok(data) => {
                    debug!("data cache miss: {:?}", cache_key);
                    match std::str::from_utf8(&data) {
                        Ok(json_str) => match serde_json::from_str::<serde_json::Value>(json_str) {
                            Ok(json) => {
                                match data_cache.insert(cache_key.clone(), json.clone()) {
                                    Ok(_) => {}
                                    Err(err) => error!(
                                        "ERROR data cache insert for {:?}: {:?}",
                                        cache_key, err
                                    ),
                                }
                                Ok(json)
                            }
                            Err(err) => Err(DataReadErr::JsonError(err)),
                        },
                        Err(err) => Err(DataReadErr::Utf8Error(err)),
                    }
                }
                Err(err) => Err(DataReadErr::IoError(err)),
            },
        }
    }
}
