use std::{ffi::OsString, fs, path::PathBuf, sync::Mutex};

use crate::{config::Config, templates::initialize_handlebars};
use cached::{Cached as _, TimedSizedCache};
use glob::glob;
use handlebars::Handlebars;
use log::debug;
use serde::Serialize;
use serde_json::json;

const MAX_FS_CACHE_ENTRIES: usize = 512;
// TODO: allow granularity by file type, images should be way higher for example
const MAX_FS_CACHE_TTL_SECONDS: u64 = 10;
const MAX_DATA_CACHE_ENTRIES: usize = 512;
const MAX_DATA_CACHE_TTL_SECONDS: u64 = 10;

#[derive(Debug)]
pub struct ServerContext {
    config: Config,
    handlebars: Mutex<Handlebars<'static>>,
    fs_cache: Mutex<TimedSizedCache<OsString, Vec<u8>>>,
    data_cache: Mutex<TimedSizedCache<OsString, serde_json::Value>>,
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
            fs_cache: Mutex::new(TimedSizedCache::with_size_and_lifespan(
                MAX_FS_CACHE_ENTRIES,
                MAX_FS_CACHE_TTL_SECONDS,
            )),
            data_cache: Mutex::new(TimedSizedCache::with_size_and_lifespan(
                MAX_DATA_CACHE_ENTRIES,
                MAX_DATA_CACHE_TTL_SECONDS,
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

        // TODO: could probably have better debug logging for errors here
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
                                let _ = handlebars.register_template_string(&partial_name, value);
                            }
                            Err(_) => (),
                        },
                        Err(_) => (),
                    }
                }
                Err(_) => {}
            }
        }
    }

    pub fn fs_read(&self, path_buf: PathBuf) -> Result<Vec<u8>, std::io::Error> {
        let cloned_path_buf = path_buf.clone();
        let cache_key = cloned_path_buf.as_os_str();
        let mut fs_cache = self.fs_cache.lock().unwrap();

        match fs_cache.cache_get(cache_key) {
            Some(data) => {
                debug!("fs cache hit: {:?}", cache_key);
                Ok(data.to_vec())
            }
            None => match fs::read(path_buf) {
                Ok(data) => {
                    debug!("fs cache miss: {:?}", cache_key);
                    fs_cache.cache_set(cache_key.into(), data.clone());
                    Ok(data)
                }
                Err(err) => Err(err),
            },
        }
    }

    pub fn get_data(&self) -> serde_json::Value {
        let base_data_path = format!("{}/", self.config().data_path());

        let mut data = json!({});

        // TODO: could probably have better debug logging for errors here
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
                        Err(_) => {}
                    }
                }
                Err(_) => {}
            }
        }

        data
    }

    fn data_read(&self, path_buf: PathBuf) -> Result<serde_json::Value, DataReadErr> {
        let cloned_path_buf = path_buf.clone();
        let cache_key = cloned_path_buf.as_os_str();
        let mut data_cache = self.data_cache.lock().unwrap();

        match data_cache.cache_get(cache_key) {
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
                                data_cache.cache_set(cache_key.into(), json.clone());
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
