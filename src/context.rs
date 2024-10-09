use std::{ffi::OsString, fs, path::PathBuf, sync::Mutex};

use crate::config::Config;
use caches::{Cache, LRUCache, RawLRU};
use log::debug;

const MAX_FS_CACHE_ENTRIES: usize = 512;

#[derive(Debug)]
pub struct ServerContext {
    config: Config,
    fs_cache: Mutex<RawLRU<OsString, Vec<u8>>>,
}

impl ServerContext {
    pub fn new_with_config(config: Config) -> ServerContext {
        ServerContext {
            config: config,
            fs_cache: Mutex::new(LRUCache::new(MAX_FS_CACHE_ENTRIES).unwrap()),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn fs_read(&self, path_buf: PathBuf) -> Result<Vec<u8>, std::io::Error> {
        let cloned_path_buf = path_buf.clone();
        let cache_key = cloned_path_buf.as_os_str();
        let mut fs_cache = self.fs_cache.lock().unwrap();

        match fs_cache.get(cache_key) {
            Some(data) => {
                debug!("cache hit: {:?}", cache_key);
                Ok(data.to_owned())
            }
            None => match fs::read(path_buf) {
                Ok(data) => {
                    debug!("cache miss: {:?}", cache_key);
                    fs_cache.put(cache_key.into(), data.clone());
                    Ok(data)
                }
                Err(err) => Err(err),
            },
        }
    }
}
