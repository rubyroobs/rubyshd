use std::{
    ffi::{OsStr, OsString},
    fs,
    path::PathBuf,
    sync::Mutex,
};

use crate::{
    config::Config,
    templates::{initialize_handlebars, DEFAULT_BLANK_PARTIAL_NAME},
};
use cached::stores::ExpiringSizedCache;
use glob::glob;
use handlebars::Handlebars;
use log::{debug, error};
use serde::Serialize;
use serde_json::json;

const MAX_FS_CACHE_ENTRIES: usize = 512;
const MAX_FS_CACHE_LONG_TTL_MS: u64 = 14_400_000;

const MAX_FS_CACHE_SHORT_TTL_EXTENSIONS: &[&str] = &["hbs", "html", "gmi", "md", "json"];
const MAX_FS_CACHE_SHORT_TTL_MS: u64 = 10_000;

const MAX_DATA_CACHE_ENTRIES: usize = 512;
const MAX_DATA_CACHE_TTL_MS: u64 = 10_000;

pub struct ServerContext {
    config: Config,
    handlebars: Mutex<Handlebars<'static>>,
    fs_cache: Mutex<ExpiringSizedCache<OsString, Vec<u8>>>,
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
        let base_data_path = format!("{}/", self.config().partials_path());

        for entry in
            glob(&format!("{}**/*.hbs", base_data_path)).expect("Failed to read data glob pattern")
        {
            match entry {
                Ok(path_buf) => {
                    let partial_name = path_buf
                        .to_str()
                        .unwrap()
                        .strip_prefix(&base_data_path)
                        .unwrap()
                        .strip_suffix(".hbs")
                        .unwrap()
                        .to_string();

                    match self.fs_read(path_buf) {
                        Ok(data) => match std::str::from_utf8(&data) {
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
                Err(err) => {
                    error!("ERROR loading JSON files by glob: {:?}", err)
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

    pub fn fs_read(&self, path_buf: PathBuf) -> Result<Vec<u8>, std::io::Error> {
        let cloned_path_buf = path_buf.clone();
        let cache_key = cloned_path_buf.as_os_str().to_os_string();
        let mut fs_cache = self.fs_cache.lock().unwrap();

        match fs_cache.get(&cache_key) {
            Some(data) => {
                debug!("fs cache hit: {:?}", cache_key);
                Ok(data.to_vec())
            }
            None => match fs::read(path_buf) {
                Ok(data) => {
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
                            data.clone(),
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
                        match fs_cache.insert(cache_key.clone(), data.clone()) {
                            Ok(_) => {}
                            Err(err) => error!(
                                "ERROR long-ttl fs cache insert for {:?}: {:?}",
                                cache_key, err
                            ),
                        }
                    }
                    Ok(data)
                }
                Err(err) => Err(err),
            },
        }
    }

    pub fn get_data(&self) -> serde_json::Value {
        let base_data_path = format!("{}/", self.config().data_path());

        let mut data = json!({});

        for entry in
            glob(&format!("{}**/*.json", base_data_path)).expect("Failed to read data glob pattern")
        {
            match entry {
                Ok(path_buf) => {
                    let data_key = path_buf
                        .to_str()
                        .unwrap()
                        .strip_prefix(&base_data_path)
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
                Err(err) => {
                    error!("ERROR loading JSON files by glob: {:?}", err)
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
