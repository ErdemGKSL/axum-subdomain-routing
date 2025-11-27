use axum::{Router, extract::Request, http::StatusCode, response::Response};
use futures::future::BoxFuture;
use lazy_static::lazy_static;
use regex::Regex;
use std::{
    collections::HashMap,
    convert::Infallible,
    sync::Arc,
    task::{Context, Poll},
};
use tower::util::ServiceExt;
use tower::{Layer, Service};

lazy_static! {
    static ref IP_REGEX: Regex = Regex::new(r"(\d+)\.(\d+)\.(\d+)\.(\d+)").unwrap();
}

const KNOWN_TLDS: &[&str] = &[
    "com", "net", "org", "tr", "edu", "gov", "io", "dev", "co", "uk", "info", "biz", "mil", "int",
    "arpa", "name", "pro", "aero", "coop", "museum", "mobi", "asia", "tel", "cat", "jobs",
    "travel", "us", "ca", "de", "fr", "au", "jp", "cn", "ru", "br", "it", "es", "nl", "se", "no",
    "fi", "dk", "pl", "ch", "be", "at",
];

/// A layer that routes requests based on the `Host` header (subdomain).
#[derive(Clone)]
pub struct SubdomainLayer {
    routes: Arc<HashMap<String, Router>>,
    strict: bool,
    known_hosts: Arc<Vec<String>>,
}

impl SubdomainLayer {
    /// Create a new `SubdomainLayer`.
    pub fn new() -> Self {
        Self {
            routes: Arc::new(HashMap::new()),
            strict: false,
            known_hosts: Arc::new(Vec::new()),
        }
    }

    /// Register a router for a specific subdomain.
    ///
    /// The `subdomain` argument is matched against the extracted subdomain from the `Host` header.
    pub fn register<S: ToString>(mut self, subdomain: S, router: Router) -> Self {
        let mut routes = (*self.routes).clone();
        routes.insert(subdomain.to_string(), router);
        self.routes = Arc::new(routes);
        self
    }

    /// Enable or disable strict subdomain checking.
    ///
    /// When strict checking is enabled, requests to unknown subdomains will return a 404 response
    /// instead of falling back to the main router.
    pub fn strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Set a list of known hosts.
    ///
    /// If the host ends with one of these known hosts, the suffix is removed to extract the subdomain.
    pub fn known_hosts(mut self, hosts: Vec<String>) -> Self {
        self.known_hosts = Arc::new(hosts);
        self
    }
}

impl Default for SubdomainLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Layer<S> for SubdomainLayer {
    type Service = SubdomainService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SubdomainService {
            inner,
            routes: self.routes.clone(),
            strict: self.strict,
            known_hosts: self.known_hosts.clone(),
        }
    }
}

/// Service that handles subdomain routing.
#[derive(Clone)]
pub struct SubdomainService<S> {
    inner: S,
    routes: Arc<HashMap<String, Router>>,
    strict: bool,
    known_hosts: Arc<Vec<String>>,
}

impl<S> Service<Request> for SubdomainService<S>
where
    S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = BoxFuture<'static, Result<Response, Infallible>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let inner = self.inner.clone();
        let routes = self.routes.clone();
        let strict = self.strict;
        let known_hosts = self.known_hosts.clone();

        // Extract host header before moving req
        let host = req
            .headers()
            .get("host")
            .and_then(|h| h.to_str().ok())
            .map(|h| h.split(':').next().unwrap_or(h).to_string());

        Box::pin(async move {
            if let Some(host) = host {
                let mut target_subdomain = None;

                // Try known hosts
                for known in known_hosts.iter() {
                    if host.ends_with(known) {
                        let remainder_len = host.len() - known.len();
                        if remainder_len > 0 && host.as_bytes()[remainder_len - 1] == b'.' {
                            target_subdomain = Some(host[..remainder_len - 1].to_string());
                            break;
                        }
                    }
                }

                if target_subdomain.is_none() {
                    let host = IP_REGEX.replace_all(&host, "$1_$2_$3_$4");
                    let parts: Vec<&str> = host.split('.').collect();
                    if !parts.is_empty() {
                        let last = *parts.last().unwrap();
                        let mut parts = parts;
                        if KNOWN_TLDS.contains(&last) {
                            parts.pop();
                        }
                        if parts.len() > 1 {
                            target_subdomain = Some(
                                parts[..parts.len() - 1]
                                    .iter()
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join("."),
                            );
                        }
                    }
                }

                if let Some(sub) = target_subdomain {
                    if let Some(router) = routes.get(&sub) {
                        return router.clone().oneshot(req).await;
                    } else if strict {
                        let response = Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(axum::body::Body::empty())
                            .unwrap();
                        return Ok(response);
                    }
                }
            }
            // Fallback to inner service
            inner.oneshot(req).await
        })
    }
}
