use apca::ApiInfo;
use futures_util::{stream::SplitStream, SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::sync_lots::{sync_trade_update, LotUpdateNotice};

type WssStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

pub type ChannelSink = async_channel::Sender<LotUpdateNotice>;

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

async fn process_json_message<'a>(
    text_msg: &str,
    update_sink: &'a ChannelSink,
) -> Option<Result<impl Send + 'a, Box<dyn std::error::Error>>> {
    tracing::info!("websocket recv: {}", text_msg);
    let resp: Result<serde_json::Value, serde_json::Error> = serde_json::from_str(&text_msg);
    match resp {
        Ok(content) => {
            if content["data"].get("event").is_some() {
                match sync_trade_update(&text_msg) {
                    Err(e) => Some(Err(e)),
                    Ok(notice) => Some(Ok(update_sink.send(notice.unwrap()))),
                }
            } else {
                None
            }
        }
        Err(e) => Some(Err(Box::new(e))),
    }
}

async fn read_messages(mut read_sink: SplitStream<WssStream>, update_sink: ChannelSink) {
    while let Some(message) = read_sink.next().await {
        let res = match message {
            Ok(Message::Ping(_msg)) => {
                tracing::info!("websocket recv: ping");
                None
            }
            Ok(Message::Pong(_msg)) => {
                tracing::info!("websocket recv: PONG");
                None
            }
            Ok(Message::Text(msg)) => {
                let text_msg = msg.to_string();
                process_json_message(&text_msg, &update_sink).await
            }
            Ok(Message::Binary(msg)) => {
                tracing::info!("websocket recv: {} bytes", msg.len());
                let text = String::from_utf8_lossy(&msg);
                process_json_message(&text, &update_sink).await
            }
            Ok(Message::Close(_msg)) => {
                // TODO reconnect, duh
                Some(Err("websocket recv: close".into()))
            }
            Err(e) => Some(Err(e.into())),
            _ => {
                tracing::info!("websocket recv: unknown {:?}", message);
                None
            }
        };

        match res {
            Some(Err(e)) => tracing::error!("error processing message: {:?}", e),
            Some(_result) => (),
            None => (),
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
