use spatial_atlas::community::{
    model,
    server::{self, App},
    store::Store,
};
#[tokio::main]
async fn main() {
    let public = std::env::var("ATLAS_PUBLIC").as_deref() == Ok("true");
    let store = if std::env::var("ATLAS_STORE").as_deref() == Ok("spanner") {
        Store::cloud().expect("cloud adapter")
    } else {
        assert!(!public, "public mode requires durable Spanner storage");
        Store::local(&std::env::var("ATLAS_COMMUNITY_DB").unwrap_or_else(|_| "community.db".into()))
            .unwrap()
    };
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8790".into())
        .parse::<u16>()
        .unwrap();
    let origin =
        std::env::var("PUBLIC_ORIGIN").unwrap_or_else(|_| format!("http://127.0.0.1:{port}"));
    assert!(!public || origin.starts_with("https://"));
    store
        .transact(|r| {
            model::seed(r);
            spatial_atlas::community::regional_profiles::apply(r)?;
            Ok(())
        })
        .await
        .expect("initialize canonical store");
    let app = App::new(store, origin, public);
    let host = if public { "0.0.0.0" } else { "127.0.0.1" };
    let listener = tokio::net::TcpListener::bind((host, port)).await.unwrap();
    println!("Spatial community listening on port {port}");
    axum::serve(listener, server::router(app)).await.unwrap();
}
