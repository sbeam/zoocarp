use axum::{
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;

use apca::api::v2::{order, orders, positions};
use apca::data::v2::last_quote;
// use apca::data::v2::Feed::IEX;
use apca::ApiInfo;
use apca::Client;
use num_decimal::Num;
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
        .route("/latest", get(get_quote))
        .route("/positions", get(get_positions))
        .route("/orders", get(get_orders))
        .route("/order", post(place_order));

    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
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

async fn get_positions() -> impl IntoResponse {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let positions = client.issue::<positions::Get>(&()).await.unwrap();

    (StatusCode::OK, Json(positions))
}

async fn get_orders() -> impl IntoResponse {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let request = orders::OrdersReq {
        status: orders::Status::Closed, // <-- TODO also need Opens
        ..orders::OrdersReq::default()
    };
    let alpaca_orders = client.issue::<orders::Get>(&request).await.unwrap();
    let orders: Vec<zoocarp::Position> = alpaca_orders
        .iter()
        .map(|o| zoocarp::Position::from(o))
        .collect();

    (StatusCode::OK, Json(orders))
}

async fn place_order(Json(input): Json<OrderPlacementInput>) -> impl IntoResponse {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let request = order::OrderReqInit {
        type_: order::Type::Limit,
        limit_price: input.limit,
        stop_price: input.stop,
        ..Default::default()
    }
    .init(
        input.sym,
        order::Side::Buy,
        order::Amount::quantity(input.qty),
    );

    let order = client.issue::<order::Post>(&request).await.unwrap();
    tracing::debug!("Created order {}", order.id.as_hyphenated());

    (StatusCode::OK, Json(order))
}

#[derive(Debug, Deserialize)]
struct OrderPlacementInput {
    sym: String,
    qty: u32,
    limit: Option<Num>,
    stop: Option<Num>,
}
