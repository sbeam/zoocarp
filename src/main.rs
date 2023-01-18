use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    routing::*,
    Json, Router,
};
use futures::future;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tower_http::cors::CorsLayer;

use apca::api::v2::{order, positions};
use apca::data::v2::{last_quote, last_trade};
// use apca::data::v2::Feed::IEX;
use apca::ApiInfo;
use apca::Client;
use num_decimal::Num;
// use apca::Error;

use dotenvy::dotenv;

pub mod sync_lots;
use sync_lots::*;

use zoocarp::*;

#[tokio::main]
async fn main() {
    dotenv().ok();
    // initialize tracing, RUST_LOG=debug
    tracing_subscriber::fmt::init();

    // Updates status/pricing of any non-final orders via API
    startup_sync().await.unwrap();

    // Subscribe to trade_updates
    listen_for_trade_updates().await.unwrap();

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
        .layer(CorsLayer::permissive());

    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn listen_for_trade_updates() -> Result<(), tungstenite::Error> {
    let api_info = ApiInfo::from_env().unwrap();

    let mut url = api_info.data_stream_base_url;
    url.set_path("/stream");

    let (socket, _response) = connect_async(url).await.expect("Can't connect");
    tracing::info!("WebSocket handshake has been successfully completed");

    let (mut writer, reader) = socket.split();

    let auth =
        json!({ "action": "auth", "key": api_info.key_id, "secret": api_info.secret }).to_string();
    writer.send(Message::text(auth)).await?;

    writer
        .send(Message::Text(
            r#"{ "action": "listen", "data": { "streams": ["trade_updates"] } }"#.into(),
        ))
        .await?;

    // put our ears on for those sweet sweet trade updates <--- lol copilot wrote this
    tokio::spawn(async move {
        reader
            .for_each(|message| async {
                if let Ok(msg) = message {
                    let msg_len = msg.len();
                    if msg.is_ping() {
                        tracing::info!("websocket recv: ping");
                    } else {
                        // Alpaca only sends binary on this channel for some reason, but playing it safe
                        let text_msg = if msg.is_binary() {
                            match msg.into_text() {
                                Ok(text) => text,
                                Err(e) => {
                                    tracing::warn!(
                                        "websocket recv {} unknown bytes, skipping: {:?}",
                                        msg_len,
                                        e
                                    );
                                    "".to_string()
                                }
                            }
                        } else {
                            msg.to_string()
                        };
                        if !text_msg.is_empty() {
                            tracing::info!("websocket recv: {}", text_msg);
                            let resp: Result<serde_json::Value, serde_json::Error> =
                                serde_json::from_str(&text_msg);
                            match resp {
                                Ok(content) => {
                                    if content["data"].get("event").is_some() {
                                        let res = sync_trade_update(&text_msg);
                                        if let Err(e) = res {
                                            tracing::error!("error syncing trade update: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("could not deserialize message: {}", e);
                                }
                            }
                        }
                    }
                }
            })
            .await;
    });

    Ok(())
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
    // TODO - filter by status (open, closed, all)
    let lots = Lot::get_lots(page, limit, show_canceled).unwrap();

    (StatusCode::OK, Json(lots))
}

#[derive(Debug, Deserialize)]
struct OrderPlacementInput {
    sym: String,
    qty: i32,
    limit: Option<Num>,
    stop: Option<Num>,
    target: Option<Num>,
    time_in_force: Option<zoocarp::OrderTimeInForce>,
    market: Option<bool>,
    side: Option<zoocarp::PositionType>,
}

async fn place_order(Json(input): Json<OrderPlacementInput>) -> impl IntoResponse {
    let side = input.side.unwrap_or(zoocarp::PositionType::Long);

    let qty = input.qty;

    let lot_id = Lot::create(
        input.sym.clone(),
        Num::from(qty),
        side,
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
            Some(zoocarp::OrderTimeInForce::Day) => order::TimeInForce::Day,
            _ => order::TimeInForce::UntilCanceled,
        },
        ..Default::default()
    }
    .init(
        input.sym,
        if side == zoocarp::PositionType::Long {
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
                ">>> Lot and order sync! {:?} {:?}",
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
                future::join_all(open_legs).await;
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
