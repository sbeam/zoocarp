use apca::ApiInfo;
use futures_util::{stream::SplitStream, SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::lot::Lot;
use crate::sync_lots::sync_trade_update;

type WssStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

type ChannelSink = async_channel::Sender<UpdateNotification>;

pub struct UpdateNotification {
    pub event: String,
    pub lot: Lot,
}

pub async fn listen_for_trade_updates(tx: ChannelSink) -> Result<(), tungstenite::Error> {
    let api_info = ApiInfo::from_env().unwrap();

    let mut url = api_info.data_stream_base_url.clone();
    url.set_path("/stream");

    match connect_and_authorize(&url, &api_info).await {
        Ok(reader) => {
            tracing::info!("Connected to trade updates stream");
            let _handle = tokio::task::spawn(read_messages(reader, tx));
        }
        Err(e) => {
            tracing::error!("Websocket connection failed: {}", e);
        }
    };

    Ok(())
}

async fn read_messages(mut read_sink: SplitStream<WssStream>, _send_sink: ChannelSink) {
    let process_json_message = |text_msg: &str| {
        tracing::info!("websocket recv: {}", text_msg);
        let resp: Result<serde_json::Value, serde_json::Error> = serde_json::from_str(&text_msg);
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
    };

    while let Some(message) = read_sink.next().await {
        match message {
            Ok(Message::Ping(_msg)) => {
                tracing::info!("websocket recv: ping");
            }
            Ok(Message::Pong(_msg)) => {
                tracing::info!("websocket recv: PONG");
            }
            Ok(Message::Text(msg)) => {
                let text_msg = msg.to_string();
                process_json_message(&text_msg);
            }
            Ok(Message::Binary(msg)) => {
                tracing::info!("websocket recv: {} bytes", msg.len());
                let text = String::from_utf8_lossy(&msg);
                process_json_message(&text);
            }
            Ok(Message::Close(_msg)) => {
                // TODO reconnect, duh
                panic!("websocket recv: CLOSE!!!!");
            }
            Err(e) => {
                tracing::error!("websocket error: {:?}", e);
            }
            _ => {
                tracing::info!("websocket recv: unknown {:?}", message);
            }
        }
    }
}

async fn connect_and_authorize(
    url: &url::Url,
    api_info: &ApiInfo,
) -> Result<SplitStream<WssStream>, tungstenite::Error> {
    let (socket, _response) = connect_async(url).await.expect("Can't connect");

    let (mut writer, reader) = socket.split();

    let auth =
        json!({ "action": "auth", "key": api_info.key_id, "secret": api_info.secret }).to_string();
    writer.send(Message::Text(auth)).await?;

    writer
        .send(Message::Text(
            r#"{ "action": "listen", "data": { "streams": ["trade_updates"] } }"#.into(),
        ))
        .await?;
    Ok(reader)
}
