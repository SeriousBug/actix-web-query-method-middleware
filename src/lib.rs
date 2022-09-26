//! An Actix Web middleware that allows you to reroute `POST` requests to other
//! methods like `PUT` or `DELETE` using a query parameter.
//!
//! This is useful in HTML forms where you can't use methods other than `GET` or
//! `POST`. By adding this middleware to your server, you can submit the form to
//! endpoints with methods other than `POST` by adding a query parameter like
//! `/your/url?_method=PUT`.
//!
//! For example, in the HTML:
//!
//! ```html
//! <form method="post" action="/path/to/endpoint?_method=DELETE">
//!   <input type="submit" value="Delete this item" />
//! </form>
//! ```
//!
//! Then in your rust code:
//!
//! ```rs
//! App::new()
//!      .wrap(QueryMethod::default())
//!      // ...
//! ```
//!
//! The middleware will strip off the `_method` query parameter when rerouting
//! your request, so the rerouting is transparent to your server code.
//!
//! Note that this middleware only applies to `POST` requests. Any other request
//! like `GET` or `HEAD` will not be changed, because it would risk opening the
//! server up to XSRF attacks. Requests like `PUT` and `DELETE` are also not
//! changed because the parameter was likely included accidentally. By default
//! the middleware will allow these requests to continue to your server
//! unchanged, but you can enable the `strict_mode` parameter to reject such
//! requests.
//!
//! The middleware will also reject any request where the method parameter
//! specifies an invalid method that Actix Web doesn't accept. You *can* use
//! custom HTTP methods like `LIST`, but not `LIST:ITEMS`. See the
//! [HTTP spec for details](https://www.w3.org/Protocols/HTTP/1.1/draft-ietf-http-v11-spec-01#Method).
//!
//! This middleware uses [tracing](https://docs.rs/tracing/latest/tracing/) for
//! logging. It will log warning events for bad requests (for example, GET
//! request with method parameter), and will log debug events for good requests
//! that have been modified by the middleware. If you prefer the `log` crate for
//! your logging, you can enable it with the `logging_log` feature. You can also
//! disable logging entirely.
//!
//! ```toml
//! # To use `log` for logging
//! actix-web-query-method-middleware = { version = "1.0", default-features = false, features = ["logging_log"] }
//! # To disable logging entirely
//! actix-web-query-method-middleware = { version = "1.0", default-features = false }
//! ```
//!
use std::future::{ready, Ready};
use std::rc::Rc;
use std::str::FromStr;

use actix_web::body::EitherBody;
use actix_web::dev::{Service, Transform};
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::http::{uri::PathAndQuery, Method, Uri};
use actix_web::{Error, HttpResponse};
use futures::future::LocalBoxFuture;
use qstring::QString;

#[derive(Clone, Debug)]
/// A middleware to pick HTTP method (PUT, DELETE, ...) with a query parameter.
///
/// This is useful for HTML forms which only support GET and POST methods. Using
/// a query parameter, you can have this middleware route the request to another
/// method.
pub struct QueryMethod {
    parameter_name: String,
    strict_mode: bool,
}

impl Default for QueryMethod {
    fn default() -> Self {
        Self {
            parameter_name: "_method".to_string(),
            strict_mode: false,
        }
    }
}

impl QueryMethod {
    /// Create the middleware with the default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// The parameter name to use. By default this is `_method`, meaning that
    /// you need to send your request like `/path?_method=POST` to use this
    /// middleware. If you happen to already use `_method` in your application,
    /// you can override the parameter name used here to pick something else.
    pub fn parameter_name(&mut self, name: &str) -> Self {
        self.parameter_name = name.to_string();
        self.to_owned()
    }

    /// Disabled by default. When enabled, the middleware will respond to
    /// non-POST requests by rejecting them with a 400 code response.
    pub fn enable_strict_mode(&mut self) -> Self {
        self.strict_mode = true;
        self.to_owned()
    }

    /// Disabled by default. When disabled, the middleware will allow non-POST
    /// requests that have
    pub fn disable_strict_mode(&mut self) -> Self {
        self.strict_mode = false;
        self.to_owned()
    }
}

impl<S: 'static, B> Transform<S, ServiceRequest> for QueryMethod
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = QueryMethodMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(QueryMethodMiddleware {
            service: Rc::new(service),
            options: self.to_owned(),
        }))
    }
}
pub struct QueryMethodMiddleware<S> {
    service: Rc<S>,
    options: QueryMethod,
}

impl<S: 'static, B> Service<ServiceRequest> for QueryMethodMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    actix_service::forward_ready!(service);

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        let uri = req.head().uri.clone();
        let mut uri_parts = uri.clone().into_parts();
        let (path, query_string) = uri_parts
            .path_and_query
            .map(|pq| {
                (
                    pq.path().to_string(),
                    pq.query()
                        .map(|q| q.to_string())
                        .unwrap_or_else(|| "".to_string()),
                )
            })
            .unwrap_or_else(|| ("".to_string(), "".to_string()));
        let query = QString::from(query_string.as_str());

        if let Some(value) = query.clone().get(&self.options.parameter_name) {
            // Method parameter specified, try to redirect
            let original_method = req.method();
            if original_method.eq(&Method::POST) {
                #[cfg(feature = "logging_tracing")]
                tracing::debug!(
                    parameter_value = value,
                    path = req.path(),
                    original_method = original_method.as_str(),
                    "Rerouting request method"
                );
                #[cfg(feature = "logging_log")]
                log::debug!("Rerouting request for {} to method {}", req.path(), value);
                match Method::from_str(value) {
                    Ok(new_method) => {
                        req.head_mut().method = new_method;
                        uri_parts.path_and_query = Some(
                            PathAndQuery::from_str(&format!(
                                "{}{}",
                                path,
                                QString::new(
                                    query
                                        .into_iter()
                                        .filter(|(k, _)| k.ne(&self.options.parameter_name))
                                        .collect::<Vec<(String, String)>>()
                                )
                            ))
                            // This unwrap is safe, since the string we're
                            // making the path an query out of is the path and
                            // query the server had already parsed and accepted.
                            // Our modification here should not break things,
                            // and we test for it as well.
                            .unwrap(),
                        );
                        // This unwrap is also safe since we're just
                        // reconstructing the uri from it's own old parts.
                        req.head_mut().uri = Uri::from_parts(uri_parts).unwrap();
                    }
                    Err(_) => {
                        #[cfg(feature = "logging_tracing")]
                        tracing::warn!(
                            parameter_name = &self.options.parameter_name,
                            parameter_value = value,
                            path = req.path(),
                            original_method = original_method.as_str(),
                            "Received a bad method query parameter"
                        );
                        #[cfg(feature = "logging_log")]
                        log::warn!(
                            "Received a bad method query parameter {} for path {}",
                            value,
                            req.path(),
                        );
                        let value = value.to_string();
                        return Box::pin(async move {
                            let response = HttpResponse::BadRequest()
                                .body(format!("Method query parameter value {} is bad", value))
                                .map_into_right_body();
                            let (request, _) = req.into_parts();
                            Ok(ServiceResponse::new(request, response))
                        });
                    }
                }
            } else {
                #[cfg(feature = "logging_tracing")]
                tracing::warn!(
                    parameter_name = &self.options.parameter_name,
                    parameter_value = value,
                    path = req.path(),
                    original_method = original_method.as_str(),
                    "Received a non-POST request with the method query parameter"
                );
                #[cfg(feature = "logging_log")]
                log::warn!(
                    "Received a {} {} request with the method query parameter",
                    original_method.as_str(),
                    req.path(),
                );
                if self.options.strict_mode {
                    let original_method = original_method.clone();
                    return Box::pin(async move {
                        let response = HttpResponse::BadRequest()
                            .body(format!(
                                "Method {} can not be rerouted with a query parameter",
                                original_method.as_str()
                            ))
                            .map_into_right_body();
                        let (request, _) = req.into_parts();
                        Ok(ServiceResponse::new(request, response))
                    });
                }
            }
        }

        let service = self.service.clone();
        Box::pin(async move {
            service
                .call(req)
                .await
                .map(ServiceResponse::map_into_left_body)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_service::ServiceFactory;
    use actix_web::{body::MessageBody, test, web, App, HttpRequest};

    fn setup_test_app() -> App<
        impl ServiceFactory<
            ServiceRequest,
            Response = ServiceResponse<impl MessageBody>,
            Config = (),
            InitError = (),
            Error = Error,
        >,
    > {
        App::new()
            .wrap(QueryMethod::new())
            .route(
                "/",
                web::get().to(|req: HttpRequest| {
                    let query_string = req.query_string().to_string();
                    async move { format!("GET {}", query_string) }
                }),
            )
            .route(
                "/",
                web::put().to(|req: HttpRequest| {
                    let query_string = req.query_string().to_string();
                    async move { format!("PUT {}", query_string) }
                }),
            )
            .route(
                "/",
                web::post().to(|req: HttpRequest| {
                    let query_string = req.query_string().to_string();
                    async move { format!("POST {}", query_string) }
                }),
            )
    }

    #[test_log::test(actix_web::test)]
    async fn test_post_rerouted() {
        let app = test::init_service(setup_test_app()).await;
        let req = test::TestRequest::post().uri("/?_method=PUT").to_request();
        let resp = test::call_and_read_body(&app, req).await;
        let resp_text = String::from_utf8_lossy(&resp[..]);
        assert_eq!(resp_text, "PUT ", "POST request rerouted to PUT");
    }

    #[test_log::test(actix_web::test)]
    async fn test_post_not_rerouted_with_query_missing() {
        let app = test::init_service(setup_test_app()).await;
        let req = test::TestRequest::post().uri("/").to_request();
        let resp = test::call_and_read_body(&app, req).await;
        let resp_text = String::from_utf8_lossy(&resp[..]);
        assert_eq!(resp_text, "POST ", "not rerouted");
    }

    #[test_log::test(actix_web::test)]
    async fn test_post_not_rerouted_with_query_different() {
        let app = test::init_service(setup_test_app()).await;
        let req = test::TestRequest::post()
            // method instead of _method
            .uri("/?method=PUT")
            .to_request();
        let resp = test::call_and_read_body(&app, req).await;
        let resp_text = String::from_utf8_lossy(&resp[..]);
        assert_eq!(resp_text, "POST method=PUT", "not rerouted");
    }

    #[test_log::test(actix_web::test)]
    async fn test_get_request_not_rerouted() {
        let app = test::init_service(setup_test_app()).await;
        let req = test::TestRequest::get().uri("/?_method=PUT").to_request();
        let resp = test::call_and_read_body(&app, req).await;
        let resp_text = String::from_utf8_lossy(&resp[..]);
        assert_eq!(resp_text, "GET _method=PUT", "not rerouted");
    }

    #[test_log::test(actix_web::test)]
    async fn test_get_request_failed_with_bad_method_value() {
        let app = test::init_service(setup_test_app()).await;
        let req = test::TestRequest::post()
            .uri("/?_method=NO:METHOD")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "Request failed due to bad method value");
    }

    #[test_log::test(actix_web::test)]
    async fn test_get_request_failed_in_strict_mode() {
        let app = test::init_service(
            App::new()
                .wrap(QueryMethod::new().enable_strict_mode())
                .route("/", web::get().to(|| async { "GET" }))
                .route("/", web::post().to(|| async { "POST" }))
                .route("/", web::put().to(|| async { "PUT" })),
        )
        .await;
        let req = test::TestRequest::get().uri("/?_method=POST").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "Request failed in strict mode");
    }

    #[test_log::test(actix_web::test)]
    async fn test_post_rerouted_with_nondefault_parameter_name() {
        let app = test::init_service(
            App::new()
                .wrap(QueryMethod::new().parameter_name("_my_hidden_method"))
                .route("/", web::get().to(|| async { "GET" }))
                .route("/", web::post().to(|| async { "POST" }))
                .route("/", web::put().to(|| async { "PUT" })),
        )
        .await;
        let req = test::TestRequest::post()
            .uri("/?_my_hidden_method=PUT")
            .to_request();
        let resp = test::call_and_read_body(&app, req).await;
        let resp_text = String::from_utf8_lossy(&resp[..]);
        assert_eq!(resp_text, "PUT", "POST request rerouted to PUT");
    }

    #[test_log::test(actix_web::test)]
    async fn test_post_not_rerouted_with_nondefault_parameter_name_and_different_query() {
        let app = test::init_service(
            App::new()
                .wrap(QueryMethod::new().parameter_name("_my_hidden_method"))
                .route("/", web::get().to(|| async { "GET" }))
                .route("/", web::post().to(|| async { "POST" }))
                .route("/", web::put().to(|| async { "PUT" })),
        )
        .await;
        let req = test::TestRequest::post()
            .uri("/?_some_other_method=PUT")
            .to_request();
        let resp = test::call_and_read_body(&app, req).await;
        let resp_text = String::from_utf8_lossy(&resp[..]);
        assert_eq!(resp_text, "POST", "not rerouted");
    }

    #[test_log::test(actix_web::test)]
    async fn test_post_reroutes_with_custom_method() {
        let app = test::init_service(
            App::new()
                .wrap(QueryMethod::new())
                .route("/", web::get().to(|| async { "GET" }))
                .route("/", web::post().to(|| async { "POST" }))
                .route(
                    "/",
                    web::method(Method::from_str("LIST").unwrap()).to(|| async { "LIST" }),
                ),
        )
        .await;
        let req = test::TestRequest::post().uri("/?_method=LIST").to_request();
        let resp = test::call_and_read_body(&app, req).await;
        let resp_text = String::from_utf8_lossy(&resp[..]);
        assert_eq!(resp_text, "LIST", "POST request rerouted to LIST");
    }

    #[test_log::test(actix_web::test)]
    async fn test_post_not_rerouted_with_bad_method_value() {
        let app = test::init_service(setup_test_app()).await;
        let req = test::TestRequest::post()
            .uri("/?_method=LIST:ITEMS")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "Bad method value is rejected");
    }
}
