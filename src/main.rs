use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    routing::*,
    Extension, Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

use apca::api::v2::{order, positions};
use apca::data::v2::{last_quote, last_trade};
// use apca::data::v2::Feed::IEX;
use apca::ApiInfo;
use apca::Client;
use num_decimal::Num;
// use apca::Error;

use dotenvy::dotenv;

use zoocarp::bucket::Bucket;
use zoocarp::lot::{self, Lot, LotStatus};
use zoocarp::sync_lots::startup_sync;
use zoocarp::trade_update_client::listen_for_trade_updates;

#[tokio::main]
async fn main() {
    dotenv().ok();
    // initialize tracing, RUST_LOG=debug
    tracing_subscriber::fmt::init();

    // create mpsc unbounded channel for trade updates with UpdateNotification
    let (update_tx, _update_rx) = async_channel::unbounded();

    // Updates status/pricing of any non-final orders via API
    startup_sync().await.unwrap();

    // Subscribe to trade_updates
    listen_for_trade_updates(update_tx).await.unwrap();

    // build our application with a route
    let app = Router::new()
        .route("/", get(root))
        .route("/latest", get(get_last_trade))
        .route("/quote/:symbol", get(get_quote))
        .route("/positions", get(get_positions))
        .route("/orders", get(get_lots))
        .route("/order", post(place_order))
        .route("/order/:id", delete(cancel_order))
        .route("/liquidate", patch(liquidate_order))
        .route("/buckets", get(list_buckets))
        .route("/bucket", post(create_bucket))
        .route("/bucket/:name", patch(update_bucket))
        .route("/bucket", delete(delete_bucket))
        // .layer(Extension(server.clone()))
        .layer(CorsLayer::permissive());

    // run it
    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

fn alpaca_client() -> Client {
    let api_info = ApiInfo::from_env().unwrap();
    Client::new(api_info)
}

fn json_error(code: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    let body = json!({ "error": msg });
    (code, Json(body))
}

// extract helpful API error responses from the apca RequestError
fn api_post_error(
    e: apca::RequestError<order::PostError>,
) -> (StatusCode, Json<serde_json::Value>) {
    match e {
        apca::RequestError::Endpoint(order::PostError::InvalidInput(api_error)) => {
            json_error(StatusCode::BAD_REQUEST, &api_error.unwrap().message)
        }
        apca::RequestError::Endpoint(order::PostError::NotPermitted(api_error)) => {
            json_error(StatusCode::BAD_REQUEST, &api_error.unwrap().message)
        }
        _ => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

// -- Handlers --
async fn root() -> impl IntoResponse {
    Json(json!({ "message": "Hello, World!" }))
}

async fn get_last_trade(Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    let req = last_trade::LastTradeRequest::new(
        params
            .get("sym")
            .unwrap()
            .split(",")
            .map(|s| s.to_string()) // ug
            .collect(),
    );
    let trade = alpaca_client()
        .issue::<last_trade::Get>(&req)
        .await
        .unwrap();

    (StatusCode::OK, Json(trade))
}

async fn get_quote(Path(symbol): Path<String>) -> impl IntoResponse {
    let req = last_quote::LastQuoteReq::new(vec![symbol]);
    let quotes = alpaca_client()
        .issue::<last_quote::Get>(&req)
        .await
        .unwrap();

    (StatusCode::OK, Json(quotes))
}

async fn get_positions() -> impl IntoResponse {
    let positions = alpaca_client().issue::<positions::Get>(&()).await.unwrap();

    (StatusCode::OK, Json(positions))
}

async fn get_lots(Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    let page = 0; // TODO: pagination
    let limit = 50;
    let show_canceled = params.contains_key("show_canceled");
    let bucket_id = params.get("bucket_id").unwrap().parse::<i64>().unwrap();
    // TODO - filter by status (open, closed, all)
    let lots = Lot::get_lots(bucket_id, page, limit, show_canceled).unwrap();

    (StatusCode::OK, Json(lots))
}

#[derive(Debug, Deserialize)]
struct OrderPlacementInput {
    sym: String,
    qty: i32,
    bucket_id: i64,
    limit: Option<Num>,
    stop: Option<Num>,
    target: Option<Num>,
    time_in_force: Option<lot::OrderTimeInForce>,
    market: Option<bool>,
    side: Option<lot::PositionType>,
}

async fn place_order(Json(input): Json<OrderPlacementInput>) -> impl IntoResponse {
    let side = input.side.unwrap_or(lot::PositionType::Long);

    let qty = input.qty;
    let bucket = Bucket::get_by_id(&input.bucket_id.into()).unwrap();

    let lot_id = Lot::create(
        input.sym.clone(),
        Num::from(qty),
        side,
        bucket,
        input.limit.clone(),
        input.target.clone(),
        input.stop.clone(),
        input.time_in_force,
    );
    let mut lot = Lot::get(lot_id).unwrap();

    let market = input.market.unwrap_or(false);

    let request = order::OrderReqInit {
        client_order_id: lot.client_id.clone(),
        class: order::Class::Bracket,
        type_: if market {
            order::Type::Market
        } else {
            order::Type::Limit
        },
        limit_price: if market { None } else { input.limit },
        // extended_hours: true, // TODO make it an input, but cannot use market, or bracket orders per docs
        stop_loss: Some(order::StopLoss::Stop(input.stop.unwrap_or_default())),
        take_profit: Some(order::TakeProfit::Limit(input.target.unwrap_or_default())),
        time_in_force: match input.time_in_force {
            Some(lot::OrderTimeInForce::Day) => order::TimeInForce::Day,
            _ => order::TimeInForce::UntilCanceled,
        },
        ..Default::default()
    }
    .init(
        input.sym,
        if side == lot::PositionType::Long {
            order::Side::Buy
        } else {
            order::Side::Sell
        },
        order::Amount::quantity(qty),
    );

    match alpaca_client().issue::<order::Post>(&request).await {
        Ok(order) => {
            tracing::debug!("Created order {}", order.id.as_hyphenated());

            lot.fill_with(&order).unwrap();
            tracing::debug!(
                ">>> New order: {:?} => {:?}",
                order.id.as_hyphenated().to_string(),
                lot.rowid
            );
            (StatusCode::OK, Json(json!(lot)))
        }
        Err(e) => {
            tracing::error!("error placing order: {:?}", e);
            api_post_error(e)
        }
    }
}

#[derive(Debug, Deserialize)]
struct OrderLiquidationInput {
    time_in_force: Option<order::TimeInForce>,
    stop: Option<Num>,
    #[serde(rename = "orderType")]
    type_: Option<order::Type>,
    id: String,
}

async fn liquidate_order(Json(input): Json<OrderLiquidationInput>) -> impl IntoResponse {
    let client_id = &input.id;

    let mut lot = Lot::get_by_client_id(&client_id).unwrap();
    if lot.status != Some(LotStatus::Open) {
        return json_error(
            StatusCode::BAD_REQUEST,
            "Lot is not open, cannont be liquidate",
        );
    }

    let id = lot.open_order_id.unwrap();
    tracing::debug!("Fetching order with id {}", id.as_hyphenated());
    let client = alpaca_client();

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
            // might want to use OCO but not clear on how to work it with a bracket order
            if retrieved.legs.len() > 1 {
                let open_legs = retrieved
                    .legs
                    .into_iter()
                    .filter(|leg| !leg.status.is_terminal())
                    .map(|leg| client.issue::<order::Delete>(&leg.id));
                futures::future::join_all(open_legs).await;
            }

            if retrieved.status.is_terminal() {
                tracing::debug!("Base order already terminal");
            } else {
                client.issue::<order::Delete>(&retrieved.id).await.unwrap();
                tracing::debug!("Deleted order {}", retrieved.id.as_hyphenated());
            }

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
            tracing::debug!("Replaced with order {}", replaced.id.as_hyphenated());

            let lot = lot.liquidate_with(&replaced).unwrap();

            (StatusCode::OK, Json(json!(lot)))
        }
    }
}

async fn cancel_order(Path(client_id): Path<String>) -> impl IntoResponse {
    let lot = Lot::get_by_client_id(&client_id).unwrap();
    if lot.status != Some(LotStatus::Pending) || lot.open_order_id.is_none() {
        return json_error(StatusCode::BAD_REQUEST, "Lot cannot be cancelled");
    }

    let response = alpaca_client()
        .issue::<order::Delete>(&lot.open_order_id.unwrap())
        .await;
    match response {
        Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
        Ok(_) => {
            // lot.cancel().unwrap();
            (StatusCode::OK, Json(json!(lot)))
        }
    }
}

async fn list_buckets() -> impl IntoResponse {
    let buckets = Bucket::list().unwrap();
    (StatusCode::OK, Json(json!(buckets)))
}

#[derive(Debug, Deserialize)]
struct BucketInput {
    name: String,
}

async fn create_bucket(Json(input): Json<BucketInput>) -> impl IntoResponse {
    let bucket = Bucket::new(&input.name).create();
    match bucket {
        Ok(b) => (StatusCode::OK, Json(json!(b))),
        Err(e) => json_error(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn update_bucket(
    Path(old_bucket_name): Path<String>,
    Json(input): Json<BucketInput>,
) -> impl IntoResponse {
    let bucket = Bucket::update_name(&old_bucket_name, &input.name);
    match bucket {
        Ok(b) => (StatusCode::OK, Json(json!(b))),
        Err(e) => json_error(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

async fn delete_bucket(Json(input): Json<BucketInput>) -> impl IntoResponse {
    let res = Bucket::delete(&input.name);
    match res {
        Ok(_) => (StatusCode::OK, Json(json!("ok"))),
        Err(e) => json_error(StatusCode::NOT_FOUND, &e.to_string()),
    }
}
