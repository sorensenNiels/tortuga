//! Custom CA certificates (R-HTTP-16), against a real HTTPS server on localhost.

use std::path::PathBuf;
use std::sync::Arc;

use rustls_pki_types::pem::PemObject as _;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tortuga_core::{Client, ClientSettings, Error, HttpMethod, Request};

const CA: &[u8] = include_bytes!("fixtures/test-ca.pem");
const CERT: &[u8] = include_bytes!("fixtures/localhost.pem");
const KEY: &[u8] = include_bytes!("fixtures/localhost.key");

/// An HTTPS server with a certificate signed by the test CA. Answers every request with 200.
async fn https_server() -> String {
    let certs = CertificateDer::pem_slice_iter(CERT)
        .collect::<Result<Vec<_>, _>>()
        .expect("certificate");
    let key = PrivateKeyDer::from_pem_slice(KEY).expect("key");
    let provider = Arc::new(tokio_rustls::rustls::crypto::aws_lc_rs::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("protocols")
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("server config");
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("address").port();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                // A client that doesn't trust the certificate hangs up here.
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match tls.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                    }
                }
                let _ = tls
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok",
                    )
                    .await;
                let _ = tls.shutdown().await;
            });
        }
    });
    format!("https://localhost:{port}/")
}

#[tokio::test]
async fn trusts_added_ca_certificates() {
    let url = https_server().await;
    let request = Request::new(HttpMethod::Get, &url);

    let err = Client::default()
        .execute(&request)
        .await
        .expect_err("unknown CA");
    assert!(
        err.to_string().to_lowercase().contains("certificate"),
        "{err}"
    );

    let files = [(PathBuf::from("company-ca.pem"), CA.to_vec())];
    let (client, problems) =
        Client::with_ca_certificates(ClientSettings::default(), &files).expect("client");
    assert!(problems.is_empty(), "{problems:?}");
    let response = client.execute(&request).await.expect("trusted now");
    assert_eq!(
        (response.status, response.body.as_slice()),
        (200, b"ok".as_slice())
    );
}

#[tokio::test]
async fn files_without_certificates_are_reported_and_the_rest_apply() {
    let url = https_server().await;
    let files = [
        (PathBuf::from("notes.txt"), b"not a certificate".to_vec()),
        (PathBuf::from("company-ca.pem"), CA.to_vec()),
    ];
    let (client, problems) =
        Client::with_ca_certificates(ClientSettings::default(), &files).expect("client");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        matches!(&problems[0], Error::CaCertificate { path, .. } if path.ends_with("notes.txt"))
    );
    assert!(problems[0].to_string().contains("notes.txt"));
    client
        .execute(&Request::new(HttpMethod::Get, &url))
        .await
        .expect("the good file still applies");
}
