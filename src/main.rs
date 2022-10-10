use axum::{
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;

use apca::data::v2::last_quote;
// use apca::data::v2::Feed::IEX;
use apca::ApiInfo;
use apca::Client;
// use apca::Error;

use chrono::DateTime;
use chrono::Utc;
use dotenvy::dotenv;

use num_decimal::Num;

#[tokio::main]
async fn main() {
    dotenv().ok();
    // initialize tracing
    tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new()
        .route("/", get(root))
        .route("/quote", get(get_quote))
        .route("/halp", get(get_halps))
        .route("/user", post(create_user));

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

#[derive(Debug, Serialize)]
struct Halps {
    pub time: DateTime<Utc>,
    pub halps: u64,
}

async fn get_halps(Query(params): Query<HashMap<String, u64>>) -> impl IntoResponse {
    // async fn get_halps(Json(pay): Json<HalpReq>) -> impl IntoResponse {
    tracing::debug!("halping!");
    let h = Halps {
        time: Utc::now(),
        halps: params.get("amount").unwrap() * 100,
    };
    (StatusCode::OK, Json(h))
}

#[derive(Debug, Serialize)]
struct SQuote {
    pub time: DateTime<Utc>,
    pub ask_price: Num,
    pub ask_size: u64,
    pub bid_price: Num,
    pub bid_size: u64,
}

// async fn get_quote(Json(payload): Json<GetQuote>) -> impl IntoResponse {
async fn get_quote(Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    tracing::debug!("quoting {}", params.get("sym").unwrap());
    let api_info = ApiInfo::from_env().unwrap();
    tracing::debug!("api keys {:?}", api_info);
    let client = Client::new(api_info);

    let req = last_quote::LastQuoteReqInit::default().init(params.get("sym").unwrap());
    let quote = client.issue::<last_quote::Get>(&req).await.unwrap();
    let sq = SQuote {
        time: quote.time,
        ask_price: quote.ask_price,
        ask_size: quote.ask_size,
        bid_price: quote.bid_price,
        bid_size: quote.bid_size,
    };

    (StatusCode::OK, Json(sq))
}

async fn create_user(
    // this argument tells axum to parse the request body
    // as JSON into a `CreateUser` type
    Json(payload): Json<CreateUser>,
) -> impl IntoResponse {
    // insert your application logic here
    let user = User {
        id: 1337,
        username: payload.username,
    };

    // this will be converted into a JSON response
    // with a status code of `201 Created`
    (StatusCode::CREATED, Json(user))
}

// the input to our `create_user` handler
#[derive(Deserialize)]
struct CreateUser {
    username: String,
}

// the input to our `create_user` handler
#[derive(Deserialize)]
struct GetQuote {
    sym: String,
}

// the output to our `create_user` handler
#[derive(Serialize)]
struct User {
    id: u64,
    username: String,
}
