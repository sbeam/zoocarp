use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use futures::future;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

use apca::api::v2::{order, orders, positions};
use apca::data::v2::{last_quote, latest_trade};
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
        .route("/quote/:symbol", get(get_quote))
        .route("/positions", get(get_positions))
        .route("/orders", get(get_lots))
        .route("/order", post(place_order))
        .route("/liquidate", patch(liquidate_order))
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

    let req = latest_trade::LatestTradeRequest::new(params.get("sym").unwrap());
    let trade = client.issue::<latest_trade::Get>(&req).await.unwrap();

    (StatusCode::OK, Json(trade))
}

async fn get_quote(Path(symbol): Path<String>) -> impl IntoResponse {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let req = last_quote::LastQuoteReq::new(symbol.as_str());
    let quotes = client.issue::<last_quote::Get>(&req).await.unwrap();

    (StatusCode::OK, Json(quotes))
}

async fn get_positions() -> impl IntoResponse {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);

    let positions = client.issue::<positions::Get>(&()).await.unwrap();

    (StatusCode::OK, Json(positions))
}

async fn get_lots() -> impl IntoResponse {
    let page = 0;
    let limit = 50;
    let lots = zoocarp::Lot::get_lots(page, limit).unwrap();
    (StatusCode::OK, Json(lots))
}

fn alpaca_client() -> Client {
    let api_info = ApiInfo::from_env().unwrap();
    Client::new(api_info)
}

fn error_as_json(code: StatusCode, msg: String) -> (StatusCode, Json<serde_json::Value>) {
    let body = json!({ "error": msg });
    (code, Json(body))
}

#[derive(Debug, Deserialize)]
struct OrderPlacementInput {
    sym: String,
    qty: u32,
    limit: Option<Num>,
    stop: Option<Num>,
    target: Option<Num>,
    time_in_force: Option<zoocarp::OrderTimeInForce>,
}

async fn place_order(Json(input): Json<OrderPlacementInput>) -> impl IntoResponse {
    let lot_id = zoocarp::Lot::create(
        input.sym.clone(),
        Num::from(input.qty),
        zoocarp::PositionType::Long,
        input.limit.clone(),
        input.target.clone(),
        input.stop.clone(),
        input.time_in_force,
    );

    tracing::debug!(">>> OK NEW LOT! id:{}", lot_id);
    // TODO bracket vs market order vs limit
    let request = order::OrderReqInit {
        client_order_id: Some(lot_id.to_string()),
        class: order::Class::Bracket,
        type_: order::Type::Limit,
        limit_price: input.limit,
        stop_loss: Some(order::StopLoss::Stop(input.stop.unwrap_or_default())),
        take_profit: Some(order::TakeProfit::Limit(input.target.unwrap_or_default())),
        time_in_force: match input.time_in_force {
            Some(zoocarp::OrderTimeInForce::Day) => order::TimeInForce::Day,
            _ => order::TimeInForce::UntilCanceled,
        },
        ..Default::default()
    }
    .init(
        input.sym,
        order::Side::Buy,
        order::Amount::quantity(input.qty),
    );

    let response = alpaca_client().issue::<order::Post>(&request).await;
    if response.is_err() {
        return error_as_json(StatusCode::BAD_REQUEST, response.err().unwrap().to_string());
    }
    let order = response.unwrap();
    tracing::debug!("Created order {}", order.id.as_hyphenated());

    let mut lot = zoocarp::Lot::get(lot_id).unwrap();

    lot.fill_with(&order).unwrap();
    tracing::debug!(
        ">>> Lot and order sync! {:?} {:?}",
        order.id.as_hyphenated(),
        lot.rowid
    );
    (StatusCode::OK, Json(json!(lot)))
}

#[derive(Debug, Deserialize)]
struct OrderLiquidationInput {
    time_in_force: Option<order::TimeInForce>,
    stop: Option<Num>,
    #[serde(rename = "orderType")]
    type_: Option<order::Type>,
    id: order::Id,
}

async fn liquidate_order(Json(input): Json<OrderLiquidationInput>) -> impl IntoResponse {
    let api_info = ApiInfo::from_env().unwrap();
    let client = Client::new(api_info);
    let id = &input.id;
    tracing::debug!("Fetching order with id {}", id.as_hyphenated());

    let get_order = client.issue::<order::Get>(&id).await;

    match get_order {
        Err(e) => {
            let body = Json(json!({
                "error": e.to_string()
            }));
            (StatusCode::NOT_FOUND, body)
        }
        Ok(retrieved) => {
            tracing::debug!("order found! {:?}", retrieved);
            let type_ = input.type_.unwrap_or(order::Type::Market);
            let stop_price = match type_ {
                order::Type::Market => None,
                _ => input.stop,
            };

            let reqt = order::OrderReqInit {
                time_in_force: input.time_in_force.unwrap_or(order::TimeInForce::Day),
                stop_price,
                type_,
                ..Default::default()
            }
            .init(
                retrieved.symbol,
                order::Side::Sell,
                order::Amount::quantity(retrieved.filled_quantity),
            );
            tracing::debug!("req! {:?}", reqt);

            let result = client.issue::<order::Post>(&reqt).await;
            if result.is_err() {
                tracing::debug!("bad! {:?}", result);
                let e = result.err().unwrap();
                let body = Json(json!({
                    "error": e.to_string()
                }));
                return (StatusCode::BAD_REQUEST, body);
            }
            let replaced = result.unwrap();

            // might want to use OCO but not clear on how to work it with a bracket order
            if retrieved.legs.len() > 1 {
                let open_legs = retrieved
                    .legs
                    .into_iter()
                    .filter(|leg| !leg.status.is_terminal())
                    .map(|leg| client.issue::<order::Delete>(&leg.id));
                future::join_all(open_legs).await;
            }

            if retrieved.status.is_terminal() {
                tracing::debug!("Base order already terminal");
            } else {
                client.issue::<order::Delete>(&retrieved.id).await.unwrap();
                tracing::debug!(
                    "Deleted order {}, new Sell order {}",
                    retrieved.id.as_hyphenated(),
                    replaced.id.as_hyphenated()
                );
            }

            (StatusCode::OK, Json(json!(replaced)))
        }
    }
}
