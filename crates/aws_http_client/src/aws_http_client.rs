use std::fmt;
use std::sync::Arc;

use aws_smithy_runtime_api::client::http::{
    HttpClient as AwsClient, HttpConnector as AwsConnector,
    HttpConnectorFuture as AwsConnectorFuture, HttpConnectorFuture, HttpConnectorSettings,
    SharedHttpConnector,
};
use aws_smithy_runtime_api::client::orchestrator::{HttpRequest as AwsHttpRequest, HttpResponse};
use aws_smithy_runtime_api::client::result::ConnectorError;
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponents;
use aws_smithy_runtime_api::http::{Headers, StatusCode};
use aws_smithy_types::body::SdkBody;
use http_client::AsyncBody;
use http_client::{HttpClient, Request};

struct AwsHttpConnector {
    client: Arc<dyn HttpClient>,
}

impl std::fmt::Debug for AwsHttpConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AwsHttpConnector").finish()
    }
}

impl AwsConnector for AwsHttpConnector {
    fn call(&self, request: AwsHttpRequest) -> AwsConnectorFuture {
        let req = match request.try_into_http1x() {
            Ok(req) => req,
            Err(err) => {
                return HttpConnectorFuture::ready(Err(ConnectorError::other(err.into(), None)));
            }
        };

        let (parts, body) = req.into_parts();

        let response = self
            .client
            .send(Request::from_parts(parts, convert_to_async_body(body)));

        HttpConnectorFuture::new(async move {
            let response = match response.await {
                Ok(response) => response,
                Err(err) => return Err(ConnectorError::other(err.into(), None)),
            };
            let (parts, body) = response.into_parts();

            let mut response = HttpResponse::new(
                StatusCode::try_from(parts.status.as_u16()).unwrap(),
                convert_to_sdk_body(body),
            );

            let headers = match Headers::try_from(parts.headers) {
                Ok(headers) => headers,
                Err(err) => return Err(ConnectorError::other(err.into(), None)),
            };

            *response.headers_mut() = headers;

            Ok(response)
        })
    }
}

#[derive(Clone)]
pub struct AwsHttpClient {
    client: Arc<dyn HttpClient>,
}

impl std::fmt::Debug for AwsHttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AwsHttpClient").finish()
    }
}

impl AwsHttpClient {
    pub fn new(client: Arc<dyn HttpClient>) -> Self {
        Self { client }
    }
}

impl AwsClient for AwsHttpClient {
    fn http_connector(
        &self,
        _settings: &HttpConnectorSettings,
        _components: &RuntimeComponents,
    ) -> SharedHttpConnector {
        SharedHttpConnector::new(AwsHttpConnector {
            client: self.client.clone(),
        })
    }
}

pub fn convert_to_sdk_body(body: AsyncBody) -> SdkBody {
    SdkBody::from_body_1_x(body)
}

pub fn convert_to_async_body(body: SdkBody) -> AsyncBody {
    match body.bytes() {
        Some(bytes) => AsyncBody::from((*bytes).to_vec()),
        None => AsyncBody::empty(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use aws_smithy_runtime_api::client::http::HttpConnector as _;
    use aws_smithy_runtime_api::client::orchestrator::HttpRequest as AwsHttpRequest;
    use aws_smithy_types::body::SdkBody;
    use futures::executor::block_on;
    use futures::future::BoxFuture;
    use futures::io::AsyncReadExt;
    use http_body::Body as _;
    use http_client::{HttpClient, Response};

    use super::{AwsHttpConnector, convert_to_async_body, convert_to_sdk_body};

    #[derive(Debug, PartialEq, Eq)]
    struct ObservedRequest {
        uri: String,
        header_value: Option<String>,
        body: Vec<u8>,
    }

    struct RecordingHttpClient {
        observed: Arc<Mutex<Option<ObservedRequest>>>,
        status: u16,
        response_header: &'static str,
        response_body: &'static str,
    }

    impl HttpClient for RecordingHttpClient {
        fn user_agent(&self) -> Option<&http::HeaderValue> {
            None
        }

        fn proxy(&self) -> Option<&http_client::Url> {
            None
        }

        fn send(
            &self,
            req: http_client::Request<http_client::AsyncBody>,
        ) -> BoxFuture<'static, anyhow::Result<Response<http_client::AsyncBody>>> {
            let observed = self.observed.clone();
            let status = self.status;
            let response_header = self.response_header;
            let response_body = self.response_body;

            Box::pin(async move {
                let (parts, mut body) = req.into_parts();
                let mut bytes = Vec::new();
                body.read_to_end(&mut bytes).await?;

                *observed.lock().unwrap() = Some(ObservedRequest {
                    uri: parts.uri.to_string(),
                    header_value: parts
                        .headers
                        .get("x-test")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    body: bytes,
                });

                Ok(Response::builder()
                    .status(status)
                    .header("x-response", response_header)
                    .body(response_body.into())
                    .unwrap())
            })
        }
    }

    async fn read_sdk_body(mut body: SdkBody) -> Vec<u8> {
        let mut bytes = Vec::new();

        loop {
            let frame =
                std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await;

            match frame {
                Some(Ok(frame)) => {
                    if let Ok(data) = frame.into_data() {
                        bytes.extend_from_slice(&data);
                    }
                }
                Some(Err(error)) => panic!("reading sdk body failed: {error}"),
                None => return bytes,
            }
        }
    }

    #[test]
    fn convert_to_async_body_preserves_bytes() {
        let mut body = convert_to_async_body(SdkBody::from("hello world"));
        let mut bytes = Vec::new();

        block_on(body.read_to_end(&mut bytes)).unwrap();

        assert_eq!(bytes, b"hello world");
    }

    #[test]
    fn convert_to_sdk_body_preserves_bytes() {
        let sdk_body = convert_to_sdk_body(http_client::AsyncBody::from("hello world"));

        assert_eq!(block_on(read_sdk_body(sdk_body)), b"hello world");
    }

    #[test]
    fn connector_converts_request_and_response() {
        let observed = Arc::new(Mutex::new(None));
        let connector = AwsHttpConnector {
            client: Arc::new(RecordingHttpClient {
                observed: observed.clone(),
                status: 201,
                response_header: "ok",
                response_body: "response-body",
            }),
        };

        let mut request = AwsHttpRequest::new(SdkBody::from("request-body"));
        request.set_uri("https://example.com/api?query=1").unwrap();
        request.headers_mut().insert("x-test", "value");

        let response = block_on(connector.call(request)).unwrap();

        assert_eq!(
            observed.lock().unwrap().as_ref(),
            Some(&ObservedRequest {
                uri: "https://example.com/api?query=1".to_string(),
                header_value: Some("value".to_string()),
                body: b"request-body".to_vec(),
            })
        );
        assert_eq!(response.status().as_u16(), 201);
        assert_eq!(response.headers().get("x-response"), Some("ok"));
        assert_eq!(
            block_on(read_sdk_body(response.into_body())),
            b"response-body"
        );
    }
}
