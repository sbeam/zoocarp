use axum::{extract::Query, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use std::collections::HashMap;
use std::net::SocketAddr;

use apca::data::v2::last_quote;
// use apca::data::v2::Feed::IEX;
use apca::ApiInfo;
use apca::Client;
// use apca::Error;

use dotenvy::dotenv;

#[tokio::main]
async fn main() {
    dotenv().ok();
    // initialize tracing
    tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new()
        .route("/", get(root))
        .route("/latest", get(get_quote));

    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Cool app, hey, World!"
}

async fn get_quote(Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let req = last_quote::LastQuoteReqInit::default().init(params.get("sym").unwrap());
    let quote = client.issue::<last_quote::Get>(&req).await.unwrap();
    let sq = zoocarp::SerializableEntityQuote::from(quote);

    (StatusCode::OK, Json(sq))
}
