use axum::{Router, routing::get};
use axum_subdomain_routing::SubdomainLayer;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_subdomain_routing() {
    // Define routers for different subdomains
    let api_router = Router::new().route("/", get(|| async { "Hello from API!" }));
    let admin_router = Router::new().route("/", get(|| async { "Hello from Admin!" }));

    // Define the main app router (fallback)
    let app = Router::new()
        .route("/", get(|| async { "Hello from Main App!" }))
        .layer(
            SubdomainLayer::new()
                .register("api", api_router)
                .register("admin", admin_router),
        );

    // Bind to a random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn the server
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();

    // Test main app
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("127.0.0.1:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from Main App!");

    // Test api subdomain
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("api.127.0.0.1:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from API!");

    // Test admin subdomain
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("admin.127.0.0.1:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from Admin!");

    // Test unknown subdomain falls back to main
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("unknown.127.0.0.1:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from Main App!");
}

#[tokio::test]
async fn test_subdomain_routing_strict() {
    // Define routers for different subdomains
    let api_router = Router::new().route("/", get(|| async { "Hello from API!" }));
    let admin_router = Router::new().route("/", get(|| async { "Hello from Admin!" }));

    // Define the main app router (fallback)
    let app = Router::new()
        .route("/", get(|| async { "Hello from Main App!" }))
        .layer(
            SubdomainLayer::new()
                .register("api", api_router)
                .register("admin", admin_router)
                .strict(true),
        );

    // Bind to a random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn the server
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();

    // Test main app still works
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("127.0.0.1:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from Main App!");

    // Test api subdomain
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("api.127.0.0.1:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from API!");

    // Test admin subdomain
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("admin.127.0.0.1:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from Admin!");

    // Test unknown subdomain returns 404 in strict mode
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("unknown.127.0.0.1:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_localhost_subdomains() {
    // Define routers for different subdomains
    let api_router = Router::new().route("/", get(|| async { "Hello from API!" }));
    let admin_router = Router::new().route("/", get(|| async { "Hello from Admin!" }));

    // Define the main app router (fallback)
    let app = Router::new()
        .route("/", get(|| async { "Hello from Main App!" }))
        .layer(
            SubdomainLayer::new()
                .register("api", api_router)
                .register("admin", admin_router),
        );

    // Bind to a random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn the server
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();

    // Test main app on localhost
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("localhost:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from Main App!");

    // Test api subdomain on localhost
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("api.localhost:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from API!");

    // Test unknown subdomain falls back
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("unknown.localhost:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from Main App!");
}

#[tokio::test]
async fn test_domain_subdomains() {
    // Define routers for different subdomains
    let api_router = Router::new().route("/", get(|| async { "Hello from API!" }));

    // Define the main app router (fallback)
    let app = Router::new()
        .route("/", get(|| async { "Hello from Main App!" }))
        .layer(SubdomainLayer::new().register("api", api_router));

    // Bind to a random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn the server
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();

    // Test main domain
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("example.com:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from Main App!");

    // Test api subdomain
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("api.example.com:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from API!");

    // Test unknown subdomain falls back
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("unknown.example.com:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from Main App!");
}

#[tokio::test]
async fn test_domain_subdomains_strict() {
    // Define routers for different subdomains
    let api_router = Router::new().route("/", get(|| async { "Hello from API!" }));

    // Define the main app router (fallback)
    let app = Router::new()
        .route("/", get(|| async { "Hello from Main App!" }))
        .layer(
            SubdomainLayer::new()
                .register("api", api_router)
                .strict(true),
        );

    // Bind to a random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn the server
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();

    // Test main domain
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("example.com:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from Main App!");

    // Test api subdomain
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("api.example.com:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from API!");

    // Test unknown subdomain returns 404
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("unknown.example.com:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_multiple_level_subdomains() {
    // Define routers for different subdomains
    let sub_api_router = Router::new().route("/", get(|| async { "Hello from Sub API!" }));

    // Define the main app router (fallback)
    let app = Router::new()
        .route("/", get(|| async { "Hello from Main App!" }))
        .layer(SubdomainLayer::new().register("sub.api", sub_api_router));

    // Bind to a random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn the server
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();

    // Test main app
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("127.0.0.1:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from Main App!");

    // Test sub.api subdomain
    let resp = client
        .get(format!("http://{}", addr))
        .header("Host", format!("sub.api.127.0.0.1:{}", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let text = resp.text().await.unwrap();
    assert_eq!(text, "Hello from Sub API!");
}
