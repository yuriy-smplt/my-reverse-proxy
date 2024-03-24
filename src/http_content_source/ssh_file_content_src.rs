use std::{sync::Arc, time::Duration};

use hyper::Uri;
use my_ssh::{SshCredentials, SshSession};
use tokio::sync::Mutex;

use crate::{app::AppContext, http_proxy_pass::ProxyPassError};

use super::{RequestExecutor, RequestExecutorResult, WebContentType};

pub struct PathOverSshContentSource {
    ssh_session: Option<Arc<SshSession>>,
    ssh_credentials: Arc<SshCredentials>,
    home_value: Arc<Mutex<Option<String>>>,
    default_file: Option<String>,
    pub file_path: String,
    execute_timeout: Duration,
}

impl PathOverSshContentSource {
    pub fn new(
        ssh_credentials: Arc<SshCredentials>,
        file_path: String,
        default_file: Option<String>,
        execute_timeout: Duration,
    ) -> Self {
        Self {
            ssh_session: None,
            file_path,
            ssh_credentials,
            home_value: Arc::new(Mutex::new(None)),
            default_file,
            execute_timeout,
        }
    }
    pub async fn connect_if_require(&mut self, app: &AppContext) -> Result<(), ProxyPassError> {
        if self.ssh_session.is_some() {
            return Ok(());
        }

        let ssh_session = Arc::new(SshSession::new(self.ssh_credentials.clone()));

        let (host, port) = self.ssh_credentials.get_host_port();
        ssh_session
            .connect_to_remote_host(host, port, app.connection_settings.remote_connect_timeout)
            .await?;

        self.ssh_session = Some(ssh_session);
        Ok(())
    }

    pub fn get_request_executor(
        &self,
        uri: &Uri,
    ) -> Result<Arc<dyn RequestExecutor + Send + Sync + 'static>, ProxyPassError> {
        if self.ssh_session.is_none() {
            return Err(ProxyPassError::ConnectionIsDisposed);
        }

        let file_path = if uri.path() == "/" {
            if let Some(default_file) = self.default_file.as_ref() {
                format!("{}/{}", self.file_path, default_file)
            } else {
                format!("{}{}", self.file_path, uri.path())
            }
        } else {
            format!("{}{}", self.file_path, uri.path())
        };

        let result = FileOverSshRequestExecutor {
            file_path,
            session: self.ssh_session.as_ref().unwrap().clone(),
            home_value: self.home_value.clone(),
            execute_timeout: self.execute_timeout,
        };
        Ok(Arc::new(result))
    }
}

pub struct FileOverSshRequestExecutor {
    session: Arc<SshSession>,
    file_path: String,
    home_value: Arc<Mutex<Option<String>>>,
    execute_timeout: Duration,
}

#[async_trait::async_trait]
impl RequestExecutor for FileOverSshRequestExecutor {
    async fn execute_request(&self) -> Result<RequestExecutorResult, ProxyPassError> {
        let file_path = if self.file_path.contains("~") {
            let mut home_value = self.home_value.lock().await;

            if home_value.is_none() {
                let home = self
                    .session
                    .execute_command("echo $HOME", self.execute_timeout)
                    .await?;
                home_value.replace(home.trim().to_string());
            }

            let result = self.file_path.replace("~", home_value.as_ref().unwrap());
            result
        } else {
            self.file_path.clone()
        };

        let result = self
            .session
            .download_remote_file(&file_path, self.execute_timeout)
            .await;

        match result {
            Ok(content) => Ok(RequestExecutorResult {
                status_code: 200,
                content_type: WebContentType::detect_by_extension(&file_path),
                body: content,
            }),
            Err(err) => {
                println!("{} -> Error: {:?}", file_path, err);
                match &err {
                    my_ssh::SshSessionError::SshError(ssh_err) => {
                        if let Some(ssh2_error) = ssh_err.as_ssh2() {
                            if let my_ssh::ssh2::ErrorCode::Session(value) = ssh2_error.code() {
                                if value == -28 {
                                    return Ok(RequestExecutorResult {
                                        status_code: 404,
                                        content_type: None,
                                        body: "Not found".as_bytes().to_vec(),
                                    });
                                }
                            }
                        }
                    }
                    _ => {}
                }

                Err(ProxyPassError::SshSessionError(err))
            }
        }
    }
}
