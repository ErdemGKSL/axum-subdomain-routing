# axum-subdomain-routing

A Tower layer for Axum that enables subdomain-based routing in Rust web applications.

## Features

- **Subdomain Routing**: Route requests based on the `Host` header subdomain
- **Known Hosts Support**: Configure known host suffixes for proper subdomain extraction
- **Strict Mode**: Optionally return 404 for unknown subdomains
- **Axum Compatible**: Seamlessly integrates with Axum's `Router`
- **IP Address Handling**: Properly handles IP addresses in host headers

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
axum-subdomain-routing = "0.0.5"
```

## Usage

Here's a basic example of how to use the `SubdomainLayer`:

```rust
use axum::{Router, routing::get};
use axum_subdomain_routing::SubdomainLayer;

#[tokio::main]
async fn main() {
    // Create your main router
    let main_router = Router::new()
        .route("/", get(|| async { "Welcome to the main site!" }));

    // Create routers for subdomains
    let api_router = Router::new()
        .route("/", get(|| async { "API endpoint" }))
        .route("/users", get(|| async { "Users API" }));

    let admin_router = Router::new()
        .route("/", get(|| async { "Admin panel" }));

    // Apply the subdomain layer
    let app = main_router.layer(
        SubdomainLayer::new()
            .register("api", api_router)
            .register("admin", admin_router)
    );

    // The server will now route:
    // - example.com/ -> main_router
    // - api.example.com/ -> api_router
    // - admin.example.com/ -> admin_router

    // Run your Axum server as usual
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### Advanced Configuration

#### Known Hosts

If your application serves multiple domains, you can specify known host suffixes:

```rust
let layer = SubdomainLayer::new()
    .known_hosts(vec!["example.com".to_string(), "example.org".to_string()])
    .register("api", api_router);
```

#### Strict Mode

Enable strict mode to return 404 for unknown subdomains:

```rust
let layer = SubdomainLayer::new()
    .strict(true)
    .register("api", api_router);
```

In strict mode, requests to unknown subdomains will return a 404 response instead of falling back to the main router.

## API Reference

### `SubdomainLayer`

The main layer struct for subdomain routing.

#### Methods

- `new() -> SubdomainLayer`: Creates a new `SubdomainLayer` instance.
- `register<S: ToString>(self, subdomain: S, router: Router) -> Self`: Registers a router for the specified subdomain.
- `strict(self, strict: bool) -> Self`: Enables or disables strict subdomain checking.
- `known_hosts(self, hosts: Vec<String>) -> Self`: Sets the list of known host suffixes.

### `SubdomainService<S>`

The service created by the layer. You typically won't interact with this directly.

## How It Works

The layer extracts the subdomain from the `Host` header of incoming requests:

1. It checks against configured known hosts to strip domain suffixes
2. Falls back to automatic TLD detection for common top-level domains
3. Handles IP addresses by replacing dots with underscores
4. Routes to the appropriate registered router or falls back to the main router

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request!

## License

This project is licensed under the MIT License - see the repository for details.
