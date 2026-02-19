use axum::{
    Router,
    extract::Request,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use std::{
    collections::HashMap,
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tower::util::ServiceExt;
use tower::{Layer, Service};
use tracing::{debug, trace};

const KNOWN_TLDS: &[&str] = &[
    "com", "net", "org", "tr", "edu", "gov", "io", "dev", "co", "uk", "info", "biz", "mil", "int",
    "arpa", "name", "pro", "aero", "coop", "museum", "mobi", "asia", "tel", "cat", "jobs",
    "travel", "us", "ca", "de", "fr", "au", "jp", "cn", "ru", "br", "it", "es", "nl", "se", "no",
    "fi", "dk", "pl", "ch", "be", "at",
];

const HOST_HEADER: &str = "host";
const X_FORWARDED_HOST_HEADER: &str = "x-forwarded-host";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HostSource {
    #[default]
    HostOnly,
    XForwardedHostFallback,
}

enum ResolvedHost {
    Present(String),
    Missing,
    Malformed,
}

fn normalize_dns_like_value(value: &str) -> Option<String> {
    let normalized = value.trim().trim_end_matches('.').to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    Some(normalized)
}

fn extract_host_without_port(host: &str) -> Option<&str> {
    if host.is_empty() {
        return None;
    }

    if let Some(rest) = host.strip_prefix('[') {
        let closing_index = rest.find(']')?;
        let ipv6_host = &rest[..closing_index];
        if ipv6_host.is_empty() {
            return None;
        }

        let trailing = &rest[closing_index + 1..];
        if trailing.is_empty() {
            return Some(ipv6_host);
        }

        if let Some(port) = trailing.strip_prefix(':')
            && !port.is_empty()
            && port.as_bytes().iter().all(u8::is_ascii_digit)
        {
            return Some(ipv6_host);
        }
        return None;
    }

    if let Some((host_without_port, port)) = host.rsplit_once(':') {
        if host_without_port.contains(':') {
            return None;
        }
        if host_without_port.is_empty() || port.is_empty() {
            return None;
        }
        if !port.as_bytes().iter().all(u8::is_ascii_digit) {
            return None;
        }
        return Some(host_without_port);
    }

    if host.contains(':') {
        return None;
    }

    Some(host)
}

fn parse_effective_host(value: &str, first_comma_only: bool) -> Option<String> {
    let candidate = if first_comma_only {
        value.split(',').next()?.trim()
    } else {
        value.trim()
    };

    let host = extract_host_without_port(candidate)?;
    normalize_dns_like_value(host)
}

fn parse_host_header(
    headers: &HeaderMap,
    name: &str,
    first_comma_only: bool,
) -> Option<Result<String, ()>> {
    let value = headers.get(name)?;
    let text = match value.to_str() {
        Ok(text) => text,
        Err(_) => return Some(Err(())),
    };
    Some(parse_effective_host(text, first_comma_only).ok_or(()))
}

fn resolve_host(headers: &HeaderMap, source: HostSource) -> ResolvedHost {
    let primary = parse_host_header(headers, HOST_HEADER, false);
    match source {
        HostSource::HostOnly => match primary {
            Some(Ok(host)) => ResolvedHost::Present(host),
            Some(Err(_)) => ResolvedHost::Malformed,
            None => ResolvedHost::Missing,
        },
        HostSource::XForwardedHostFallback => {
            if let Some(Ok(host)) = primary {
                return ResolvedHost::Present(host);
            }

            let fallback = parse_host_header(headers, X_FORWARDED_HOST_HEADER, true);
            match fallback {
                Some(Ok(host)) => ResolvedHost::Present(host),
                Some(Err(_)) => ResolvedHost::Malformed,
                None => {
                    if matches!(primary, Some(Err(_))) {
                        ResolvedHost::Malformed
                    } else {
                        ResolvedHost::Missing
                    }
                }
            }
        }
    }
}

fn normalize_subdomain_key(subdomain: &str) -> String {
    subdomain.trim().trim_matches('.').to_ascii_lowercase()
}

fn collapse_trailing_ipv4(host: &str) -> String {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() < 4 {
        return host.to_string();
    }

    let octets = &parts[parts.len() - 4..];
    let is_ipv4_suffix = octets
        .iter()
        .all(|part| !part.is_empty() && part.parse::<u8>().is_ok());

    if !is_ipv4_suffix {
        return host.to_string();
    }

    let ipv4_collapsed = octets.join("_");
    if parts.len() == 4 {
        return ipv4_collapsed;
    }

    format!("{}.{}", parts[..parts.len() - 4].join("."), ipv4_collapsed)
}

fn not_found_response() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(axum::body::Body::empty())
        .unwrap()
}

/// A layer that routes requests based on the `Host` header (subdomain).
#[derive(Clone)]
pub struct SubdomainLayer {
    routes: Arc<HashMap<String, Router>>,
    strict: bool,
    known_hosts: Arc<Vec<String>>,
    auto_detect_domain: bool,
    host_source: HostSource,
}

impl SubdomainLayer {
    /// Create a new `SubdomainLayer`.
    pub fn new() -> Self {
        Self {
            routes: Arc::new(HashMap::new()),
            strict: false,
            known_hosts: Arc::new(Vec::new()),
            auto_detect_domain: true,
            host_source: HostSource::default(),
        }
    }

    /// Register a router for a specific subdomain.
    ///
    /// The `subdomain` argument is matched against the extracted subdomain from the `Host` header.
    pub fn register<S: ToString>(mut self, subdomain: S, router: Router) -> Self {
        let subdomain_str = normalize_subdomain_key(&subdomain.to_string());
        debug!(subdomain = %subdomain_str, "Registering router for subdomain");
        let mut routes = (*self.routes).clone();
        routes.insert(subdomain_str, router);
        self.routes = Arc::new(routes);
        self
    }

    /// Enable or disable strict subdomain checking.
    ///
    /// When strict checking is enabled, requests to unknown subdomains will return a 404 response
    /// instead of falling back to the main router.
    pub fn strict(mut self, strict: bool) -> Self {
        debug!(strict = %strict, "Setting strict mode");
        self.strict = strict;
        self
    }

    /// Set a list of known hosts.
    ///
    /// If the host ends with one of these known hosts, the suffix is removed to extract the subdomain.
    pub fn known_hosts(mut self, hosts: Vec<String>) -> Self {
        let normalized_hosts: Vec<String> = hosts
            .into_iter()
            .filter_map(|host| normalize_dns_like_value(&host))
            .collect();
        debug!(hosts = ?normalized_hosts, "Setting known hosts");
        self.known_hosts = Arc::new(normalized_hosts);
        self
    }

    /// Enable or disable automatic domain detection.
    ///
    /// When enabled, the layer will attempt to automatically detect and strip known TLDs.
    pub fn auto_detect_domain(mut self, enable: bool) -> Self {
        debug!(auto_detect = %enable, "Setting auto-detect domain mode");
        self.auto_detect_domain = enable;
        self
    }

    /// Configure how host should be resolved.
    ///
    /// By default, only the `Host` header is used.
    pub fn host_source(mut self, host_source: HostSource) -> Self {
        debug!(?host_source, "Setting host source mode");
        self.host_source = host_source;
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
            auto_detect_domain: self.auto_detect_domain,
            known_hosts: self.known_hosts.clone(),
            host_source: self.host_source,
        }
    }
}

/// Service that handles subdomain routing.
#[derive(Clone)]
pub struct SubdomainService<S> {
    inner: S,
    routes: Arc<HashMap<String, Router>>,
    strict: bool,
    auto_detect_domain: bool,
    known_hosts: Arc<Vec<String>>,
    host_source: HostSource,
}

impl<S> Service<Request> for SubdomainService<S>
where
    S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let inner = self.inner.clone();
        let routes = self.routes.clone();
        let strict = self.strict;
        let auto_detect_domain = self.auto_detect_domain;
        let known_hosts = self.known_hosts.clone();
        let host_source = self.host_source;

        let resolved_host = resolve_host(req.headers(), host_source);

        Box::pin(async move {
            match resolved_host {
                ResolvedHost::Malformed => {
                    trace!("Malformed effective host detected");
                    if strict {
                        return Ok(not_found_response());
                    }
                    trace!(
                        "Falling back to inner service due to malformed host in non-strict mode"
                    );
                    return inner.oneshot(req).await;
                }
                ResolvedHost::Missing => {
                    trace!("No effective host header found");
                    if strict {
                        return Ok(not_found_response());
                    }
                    trace!("Falling back to inner service due to missing host in non-strict mode");
                    return inner.oneshot(req).await;
                }
                ResolvedHost::Present(host) => {
                    debug!(host = %host, "Processing request for host");
                    let mut target_subdomain = None;

                    // Try known hosts
                    trace!(known_hosts = ?known_hosts.as_ref(), "Checking against known hosts");
                    for known in known_hosts.iter() {
                        if host.ends_with(known) {
                            trace!(known_host = %known, "Host matches known host");
                            let remainder_len = host.len() - known.len();
                            if remainder_len > 0 && host.as_bytes()[remainder_len - 1] == b'.' {
                                let subdomain = host[..remainder_len - 1].to_string();
                                debug!(subdomain = %subdomain, known_host = %known, "Extracted subdomain from known host");
                                target_subdomain = Some(subdomain);
                                break;
                            }
                        }
                    }

                    if target_subdomain.is_none() && auto_detect_domain {
                        trace!("Attempting auto-detection of subdomain");
                        let collapsed_host = collapse_trailing_ipv4(&host);
                        let mut parts: Vec<&str> = collapsed_host.split('.').collect();
                        trace!(parts = ?parts, "Split host into parts");
                        if !parts.is_empty() {
                            let last = *parts.last().unwrap();
                            if KNOWN_TLDS.contains(&last) {
                                trace!(tld = %last, "Detected known TLD");
                                parts.pop();
                            }
                            if parts.len() > 1 {
                                let subdomain = parts[..parts.len() - 1].to_vec().join(".");
                                debug!(subdomain = %subdomain, "Auto-detected subdomain");
                                target_subdomain = Some(subdomain);
                            }
                        }
                    }

                    if let Some(sub) = target_subdomain {
                        if let Some(router) = routes.get(&sub) {
                            debug!(subdomain = %sub, "Routing to registered subdomain router");
                            return router.clone().oneshot(req).await;
                        } else if strict {
                            debug!(subdomain = %sub, "Subdomain not found, returning 404 (strict mode)");
                            return Ok(not_found_response());
                        } else {
                            debug!(subdomain = %sub, "Subdomain not found, falling back to inner service");
                        }
                    } else {
                        trace!("No subdomain detected");
                    }
                }
            }
            // Fallback to inner service
            trace!("Falling back to inner service");
            inner.oneshot(req).await
        })
    }
}
