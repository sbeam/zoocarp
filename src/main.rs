use axum::{
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::future;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

use apca::api::v2::{order, orders, positions};
use apca::data::v2::latest_trade;
// use apca::data::v2::Feed::IEX;
use apca::ApiInfo;
use apca::Client;
use num_decimal::Num;
// use apca::Error;

use dotenvy::dotenv;

#[tokio::main]
async fn main() {
    dotenv().ok();
    // initialize tracing, RUST_LOG=debug
    tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new()
        .route("/", get(root))
        .route("/latest", get(get_latest_trade))
        .route("/positions", get(get_positions))
        .route("/orders", get(get_orders))
        .route("/order", post(place_order))
        .layer(CorsLayer::permissive());

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

async fn get_latest_trade(Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let req = latest_trade::LatestTradeRequestInit::default().init(params.get("sym").unwrap());
    let trade = client.issue::<latest_trade::Get>(&req).await.unwrap();

    (StatusCode::OK, Json(trade))
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
    let statuses = [orders::Status::Closed, orders::Status::Open];

    // credit where due, after 1000 attempts to make 2 requests to API concurrent,
    // this was the only way that worked https://stackoverflow.com/questions/51044467/how-can-i-perform-parallel-asynchronous-http-get-requests-with-reqwest
    // TODO and not even necessary bc can just query orders::Status::All
    let reqs = future::join_all(statuses.into_iter().map(|status| {
        let client = &client;
        async move {
            let request = orders::OrdersReq {
                status,
                ..orders::OrdersReq::default()
            };
            client.issue::<orders::Get>(&request).await.unwrap()
        }
    }))
    .await;

    let mut positions: Vec<zoocarp::Position> = vec![];

    for orders in reqs {
        tracing::debug!("{:?}", orders);
        orders
            .into_iter()
            .filter(|order| {
                [
                    order::Status::New,
                    order::Status::Filled,
                    order::Status::PartiallyFilled,
                ]
                .contains(&order.status)
            })
            .for_each(|o| positions.push(zoocarp::Position::from(&o)))
    }

    // reverse sort orders by created_at
    positions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    (StatusCode::OK, Json(positions))
}

async fn place_order(Json(input): Json<OrderPlacementInput>) -> impl IntoResponse {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    // TODO bracket vs market order vs limit
    let request = order::OrderReqInit {
        class: order::Class::Bracket,
        type_: order::Type::Limit,
        limit_price: input.limit,
        stop_loss: Some(order::StopLoss::Stop(input.stop.unwrap_or_default())),
        take_profit: Some(order::TakeProfit::Limit(input.target.unwrap_or_default())),
        ..Default::default()
    }
    .init(
        input.sym,
        order::Side::Buy,
        order::Amount::quantity(input.qty),
    );

    let response = client.issue::<order::Post>(&request).await;
    if response.is_err() {
        let body = Json(json!({
            "error": response.err().unwrap().to_string()
        }));
        return (StatusCode::BAD_REQUEST, body);
    }
    let order = response.unwrap();

    tracing::debug!("Created order {}", order.id.as_hyphenated());

    (StatusCode::OK, Json(json!(order)))
}

#[derive(Debug, Deserialize)]
struct OrderPlacementInput {
    sym: String,
    qty: u32,
    limit: Option<Num>,
    stop: Option<Num>,
    target: Option<Num>,
}
