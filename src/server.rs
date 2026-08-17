use base64::{Engine as _, engine::general_purpose::STANDARD};
use bytes::Bytes;
use http::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use http_body_util::Full;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

pub(crate) type BoxError = Box<dyn std::error::Error + Send + Sync>;
pub(crate) type ResponseBody = Full<Bytes>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Authorization {
    None,
    #[allow(dead_code)]
    Basic(String),
}

#[derive(Debug, Clone)]
pub(crate) struct ServerOptions {
    pub(crate) addr: SocketAddr,
    pub(crate) authorization: Authorization,
}

async fn bind_listener(addr: SocketAddr) -> io::Result<TcpListener> {
    TcpListener::bind(addr).await
}

fn response(status: StatusCode, body: impl Into<Bytes>) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .body(Full::new(body.into()))
        .expect("valid HTTP response")
}

fn unauthorized() -> Response<ResponseBody> {
    response(StatusCode::UNAUTHORIZED, Bytes::new())
}

fn is_authorized<B>(request: &Request<B>, authorization: &Authorization) -> bool {
    match authorization {
        Authorization::None => true,
        Authorization::Basic(password) => {
            let Some(header) = request
                .headers()
                .get(HeaderName::from_static("authorization"))
            else {
                return false;
            };
            let Ok(header) = header.to_str() else {
                return false;
            };
            let mut parts = header.split(' ');
            let (Some(scheme), Some(credentials), None) =
                (parts.next(), parts.next(), parts.next())
            else {
                return false;
            };
            if scheme != "Basic" {
                return false;
            }

            let Ok(decoded) = STANDARD.decode(credentials) else {
                return false;
            };
            let Ok(decoded) = String::from_utf8(decoded) else {
                return false;
            };
            decoded == format!(":{password}")
        }
    }
}

pub(crate) async fn handle_request<O, F, Fut, B>(
    server_options: Arc<ServerOptions>,
    request: Request<B>,
    callback: F,
    options: Arc<O>,
) -> Response<ResponseBody>
where
    F: Fn(Arc<O>) -> Fut,
    Fut: Future<Output = Result<String, BoxError>>,
{
    if !is_authorized(&request, &server_options.authorization) {
        return unauthorized();
    }
    if request.uri().path() != "/metrics" {
        return response(StatusCode::NOT_FOUND, Bytes::new());
    }
    if request.method() != hyper::Method::GET {
        return response(StatusCode::METHOD_NOT_ALLOWED, Bytes::new());
    }

    match callback(options).await {
        Ok(metrics) => Response::builder()
            .status(StatusCode::OK)
            .header(
                CONTENT_TYPE,
                HeaderValue::from_static("text/plain; version=0.0.4"),
            )
            .body(Full::new(Bytes::from(metrics)))
            .expect("valid HTTP response"),
        Err(error) => {
            log::warn!("failed to generate metrics: {error}");
            response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
        }
    }
}

pub(crate) async fn run_server<O, F, Fut>(
    server_options: ServerOptions,
    options: O,
    callback: F,
) -> io::Result<()>
where
    O: Send + Sync + 'static,
    F: Fn(Arc<O>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<String, BoxError>> + Send + 'static,
{
    let listener = bind_listener(server_options.addr).await?;
    let server_options = Arc::new(server_options);
    let options = Arc::new(options);

    loop {
        let (stream, _) = match listener.accept().await {
            Ok(connection) => connection,
            Err(error) => {
                log::warn!("failed to accept connection: {error}");
                continue;
            }
        };
        let server_options = Arc::clone(&server_options);
        let options = Arc::clone(&options);
        let callback = callback.clone();

        tokio::spawn(async move {
            if let Err(error) = serve_connection(stream, server_options, options, callback).await {
                log::warn!("failed to serve connection: {error}");
            }
        });
    }
}

async fn serve_connection<O, F, Fut>(
    stream: TcpStream,
    server_options: Arc<ServerOptions>,
    options: Arc<O>,
    callback: F,
) -> Result<(), hyper::Error>
where
    O: Send + Sync + 'static,
    F: Fn(Arc<O>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<String, BoxError>> + Send + 'static,
{
    let io = TokioIo::new(stream);
    let service = service_fn(move |request| {
        let server_options = Arc::clone(&server_options);
        let options = Arc::clone(&options);
        let callback = callback.clone();

        async move {
            Ok::<_, Infallible>(handle_request(server_options, request, callback, options).await)
        }
    });

    hyper::server::conn::http1::Builder::new()
        .serve_connection(io, service)
        .await
}

#[cfg(test)]
async fn serve_one_connection<O, F, Fut>(
    listener: TcpListener,
    server_options: Arc<ServerOptions>,
    options: Arc<O>,
    callback: F,
) -> Result<(), BoxError>
where
    O: Send + Sync + 'static,
    F: Fn(Arc<O>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<String, BoxError>> + Send + 'static,
{
    let (stream, _) = listener.accept().await?;
    serve_connection(stream, server_options, options, callback).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue};
    use http_body_util::{BodyExt, Empty};
    use hyper::{Method, Request, Response, StatusCode};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};

    type BoxError = Box<dyn std::error::Error + Send + Sync>;

    fn server_options(authorization: Authorization) -> Arc<ServerOptions> {
        Arc::new(ServerOptions {
            addr: "127.0.0.1:0".parse().unwrap(),
            authorization,
        })
    }

    fn request(method: Method, path: &str) -> Request<Empty<Bytes>> {
        Request::builder()
            .method(method)
            .uri(path)
            .body(Empty::new())
            .unwrap()
    }

    fn request_with_authorization(value: HeaderValue) -> Request<Empty<Bytes>> {
        let mut request = request(Method::GET, "/metrics");
        request.headers_mut().insert(AUTHORIZATION, value);
        request
    }

    async fn response_body(response: Response<ResponseBody>) -> Bytes {
        response.into_body().collect().await.unwrap().to_bytes()
    }

    #[tokio::test]
    async fn serves_one_real_http_request() {
        let listener = bind_listener("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            serve_one_connection(
                listener,
                server_options(Authorization::None),
                Arc::new(()),
                |_| async { Ok::<_, BoxError>("metric 1\n".to_owned()) },
            )
            .await
        });

        let mut stream = TcpStream::connect(address).await.unwrap();
        stream
            .write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
            .await
            .unwrap()
            .unwrap();
        let response = String::from_utf8(response).unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();

        assert_eq!(headers.lines().next(), Some("HTTP/1.1 200 OK"));
        assert!(headers.lines().any(|header| {
            header.eq_ignore_ascii_case("content-type: text/plain; version=0.0.4")
        }));
        assert_eq!(body, "metric 1\n");

        timeout(Duration::from_secs(5), server_task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn binds_tcp_listener() {
        let listener = bind_listener("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let local_addr = listener.local_addr().unwrap();

        assert!(local_addr.ip().is_loopback());
        assert_ne!(local_addr.port(), 0);
    }

    #[tokio::test]
    async fn serves_metrics_for_get() {
        let request = request(Method::GET, "/metrics");

        let response = handle_request(
            server_options(Authorization::None),
            request,
            |_| async { Ok::<_, BoxError>("metric 1\n".to_owned()) },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[CONTENT_TYPE],
            "text/plain; version=0.0.4"
        );
        assert_eq!(response_body(response).await, "metric 1\n");
    }

    #[tokio::test]
    async fn rejects_unknown_path() {
        let callback_invoked = Arc::new(AtomicBool::new(false));
        let callback_invoked_for_callback = Arc::clone(&callback_invoked);
        let response = handle_request(
            server_options(Authorization::None),
            request(Method::GET, "/health"),
            move |_| {
                callback_invoked_for_callback.store(true, Ordering::SeqCst);
                async { Ok::<_, BoxError>("unexpected".to_owned()) }
            },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(!callback_invoked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn rejects_non_get_method() {
        let callback_invoked = Arc::new(AtomicBool::new(false));
        let callback_invoked_for_callback = Arc::clone(&callback_invoked);
        let response = handle_request(
            server_options(Authorization::None),
            request(Method::POST, "/metrics"),
            move |_| {
                callback_invoked_for_callback.store(true, Ordering::SeqCst);
                async { Ok::<_, BoxError>("unexpected".to_owned()) }
            },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(!callback_invoked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn returns_internal_server_error() {
        let request = request(Method::GET, "/metrics");

        let response = handle_request(
            server_options(Authorization::None),
            request,
            |_| async { Err::<String, BoxError>("metric generation failed".into()) },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(response_body(response).await, "metric generation failed");
    }

    #[tokio::test]
    async fn basic_auth_missing_header_returns_unauthorized() {
        let response = handle_request(
            server_options(Authorization::Basic("secret".to_owned())),
            request(Method::GET, "/metrics"),
            |_| async { Ok::<_, BoxError>("unexpected".to_owned()) },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn basic_auth_malformed_header_returns_unauthorized() {
        let response = handle_request(
            server_options(Authorization::Basic("secret".to_owned())),
            request_with_authorization(HeaderValue::from_static("Basic")),
            |_| async { Ok::<_, BoxError>("unexpected".to_owned()) },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn basic_auth_invalid_header_value_returns_unauthorized() {
        let value = HeaderValue::from_bytes(b"Basic \xff").unwrap();
        let response = handle_request(
            server_options(Authorization::Basic("secret".to_owned())),
            request_with_authorization(value),
            |_| async { Ok::<_, BoxError>("unexpected".to_owned()) },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn basic_auth_extra_token_returns_unauthorized() {
        let response = handle_request(
            server_options(Authorization::Basic("secret".to_owned())),
            request_with_authorization(HeaderValue::from_static("Basic token extra")),
            |_| async { Ok::<_, BoxError>("unexpected".to_owned()) },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn basic_auth_rejects_non_literal_space_formatting() {
        for value in [
            "Basic\tOnNlY3JldA==",
            "Basic  OnNlY3JldA==",
            " Basic OnNlY3JldA==",
            "Basic OnNlY3JldA== ",
        ] {
            let response = handle_request(
                server_options(Authorization::Basic("secret".to_owned())),
                request_with_authorization(HeaderValue::from_static(value)),
                |_| async { Ok::<_, BoxError>("unexpected".to_owned()) },
                Arc::new(()),
            )
            .await;

            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{value:?}");
        }
    }

    #[tokio::test]
    async fn basic_auth_non_basic_scheme_returns_unauthorized() {
        let response = handle_request(
            server_options(Authorization::Basic("secret".to_owned())),
            request_with_authorization(HeaderValue::from_static("Bearer token")),
            |_| async { Ok::<_, BoxError>("unexpected".to_owned()) },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn basic_auth_invalid_base64_returns_unauthorized() {
        let response = handle_request(
            server_options(Authorization::Basic("secret".to_owned())),
            request_with_authorization(HeaderValue::from_static("Basic not-base64")),
            |_| async { Ok::<_, BoxError>("unexpected".to_owned()) },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn basic_auth_invalid_utf8_returns_unauthorized() {
        let value = HeaderValue::from_static("Basic //4=");
        let response = handle_request(
            server_options(Authorization::Basic("secret".to_owned())),
            request_with_authorization(value),
            |_| async { Ok::<_, BoxError>("unexpected".to_owned()) },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn basic_auth_wrong_password_returns_unauthorized() {
        let response = handle_request(
            server_options(Authorization::Basic("secret".to_owned())),
            request_with_authorization(HeaderValue::from_static("Basic d3Jvbmc=")),
            |_| async { Ok::<_, BoxError>("unexpected".to_owned()) },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn basic_auth_valid_secret_reaches_callback() {
        let callback_invoked = Arc::new(AtomicBool::new(false));
        let callback_invoked_for_callback = Arc::clone(&callback_invoked);
        let response = handle_request(
            server_options(Authorization::Basic("secret".to_owned())),
            request_with_authorization(HeaderValue::from_static("Basic OnNlY3JldA==")),
            move |_| {
                callback_invoked_for_callback.store(true, Ordering::SeqCst);
                async { Ok::<_, BoxError>("metric 1\n".to_owned()) }
            },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert!(callback_invoked.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn basic_auth_valid_credentials_still_route_unknown_path() {
        let mut request = request(Method::GET, "/unknown");
        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_static("Basic OnNlY3JldA=="),
        );

        let response = handle_request(
            server_options(Authorization::Basic("secret".to_owned())),
            request,
            |_| async { Ok::<_, BoxError>("unexpected".to_owned()) },
            Arc::new(()),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
