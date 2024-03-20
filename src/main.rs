use std::sync::Arc;

use app::AppContext;

mod app;
mod flows;
//mod http2_executor;
mod http_client;
mod http_server;
mod settings;

mod tcp_port_forward;

#[tokio::main]
async fn main() {
    let settings_reader = settings::SettingsReader::new(".my-reverse-proxy").await;

    let listen_ports = settings_reader.get_listen_ports().await;

    let app = AppContext::new(settings_reader).await;

    let app = Arc::new(app);

    for (listen_port, endpoint_type) in listen_ports {
        let listen_end_point = std::net::SocketAddr::from(([0, 0, 0, 0], listen_port));

        match endpoint_type {
            settings::EndpointType::Http1 => {
                crate::http_server::start_http_server(listen_end_point, app.clone());
            }
            settings::EndpointType::Https1(certificate_id) => {
                if let Some(ssl_certificate) = app
                    .settings_reader
                    .get_ssl_certificate(&certificate_id)
                    .await
                {
                    crate::http_server::start_https_server(
                        listen_end_point,
                        app.clone(),
                        ssl_certificate,
                    );
                } else {
                    panic!(
                        "Certificate not found: {} for endpoint: {}",
                        certificate_id.as_str(),
                        listen_port
                    );
                }
            }
            settings::EndpointType::Http2 => {
                crate::http_server::start_http2_server(listen_end_point, app.clone());
            }
            settings::EndpointType::Tcp(remote_addr) => {
                crate::tcp_port_forward::start_tcp(app.clone(), listen_end_point, remote_addr);
            }
            settings::EndpointType::TcpOverSsh(ssh_configuration) => {
                crate::tcp_port_forward::start_tcp_over_ssh(
                    app.clone(),
                    listen_end_point,
                    ssh_configuration,
                );
            }
        }
    }

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
