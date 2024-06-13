use std::{collections::HashMap, time::Duration};

use my_settings_reader::flurl::FlUrl;
use my_ssh::SshSession;
use rust_extensions::StrOrString;

use crate::settings::LocalFilePath;

use super::{SshConfigSettings, SshConfiguration};

pub enum FileSource {
    File(String),
    Http(String),
    Ssh(SshConfiguration),
}

impl FileSource {
    pub fn from_src(
        src: StrOrString,
        ssh_config: &Option<HashMap<String, SshConfigSettings>>,
    ) -> Result<Self, String> {
        if src.as_str().starts_with("http") {
            return Ok(FileSource::Http(src.to_string()));
        }

        if src.as_str().starts_with(super::SSH_PREFIX) {
            return Ok(FileSource::Ssh(SshConfiguration::parse(
                src.as_str(),
                ssh_config,
            )?));
        }

        Ok(Self::File(src.to_string()))
    }

    pub fn as_str<'s>(&'s self) -> StrOrString<'s> {
        match self {
            FileSource::File(s) => s.into(),
            FileSource::Http(s) => s.into(),
            FileSource::Ssh(s) => format!(
                "{}->{}",
                s.credentials.to_string(),
                s.remote_content.as_str()
            )
            .into(),
        }
    }

    pub async fn load_file_content(&self) -> Vec<u8> {
        match self {
            FileSource::File(file_name) => {
                println!("Loading file {}", file_name);
                let file_name = LocalFilePath::new(file_name.to_string());

                let result = tokio::fs::read(file_name.get_value().as_str())
                    .await
                    .unwrap();

                result
            }
            FileSource::Http(path) => {
                let response = FlUrl::new(path).get().await.unwrap();
                let result = response.receive_body().await.unwrap();
                result
            }
            FileSource::Ssh(ssh_credentials) => match &ssh_credentials.remote_content {
                crate::settings::SshContent::RemoteHost(_) => {
                    panic!("Reading file is not supported from socket yet");
                }
                crate::settings::SshContent::FilePath(path) => {
                    println!(
                        "Loading file {}->{}",
                        ssh_credentials.credentials.to_string(),
                        path
                    );
                    let ssh_session = SshSession::new(ssh_credentials.credentials.clone().into());

                    let result = ssh_session
                        .download_remote_file(&path, Duration::from_secs(5))
                        .await
                        .unwrap();

                    result
                }
            },
        }
    }
}
