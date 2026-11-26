//! How the page in Spotify finds the service: a port on 127.0.0.1 and a
//! secret, written once in `<data>/service/gallery.json` and put into the
//! payload at every apply.
//!
//! The secret lives in files only the user can read (their profile), like
//! the rest of spacetify's files, so the service only answers the page that
//! spacetify injected.

use std::fs;
use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::backup::write_atomic;
use crate::service::ServicePaths;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub port: u16,
    pub token: String,
}

impl Link {
    pub fn file(paths: &ServicePaths) -> PathBuf {
        paths.dir.join("gallery.json")
    }

    /// The saved link, if there is a valid one.
    pub fn load(paths: &ServicePaths) -> Option<Self> {
        let link: Self = serde_json::from_slice(&fs::read(Self::file(paths)).ok()?).ok()?;
        (link.port != 0 && link.token.len() == 32).then_some(link)
    }

    pub fn load_or_create(paths: &ServicePaths) -> Result<Self> {
        match Self::load(paths) {
            Some(link) => Ok(link),
            None => Self::create(paths),
        }
    }

    /// A new port and secret, saved. The port is free right now.
    pub fn create(paths: &ServicePaths) -> Result<Self> {
        let path = Self::file(paths);
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .and_then(|listener| listener.local_addr())
            .map_err(Error::io(&path))?
            .port();
        let mut secret = [0u8; 16];
        getrandom::fill(&mut secret).map_err(|err| Error::Setup(err.to_string()))?;
        let link = Self {
            port,
            token: secret.iter().map(|b| format!("{b:02x}")).collect(),
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(Error::io(dir))?;
        }
        let json = serde_json::to_vec_pretty(&link).expect("link is serializable");
        write_atomic(&path, &json)?;
        Ok(link)
    }

    /// For the payload: the link, or none (no gallery) if it cannot be made.
    pub fn for_payload(paths: &ServicePaths) -> Option<Self> {
        Self::load_or_create(paths)
            .inspect_err(|err| tracing::warn!("no gallery: {err}"))
            .ok()
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}
