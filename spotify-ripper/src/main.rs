use axum::{
    http::{Response, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Extension, Json, Router,
};
use log::{debug, error, info, log_enabled, Level};
use rspotify::{
    model::{AlbumId, Market},
    prelude::*,
    ClientCredsSpotify, Config, Credentials,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc, time::Duration};

use google_cloud_googleapis::pubsub::v1::PubsubMessage;
use google_cloud_pubsub::subscription::SubscriptionConfig;
use google_cloud_pubsub::topic::TopicConfig;

use pub_sub_client::{PubSubClient, PublishedMessage, RawPublishedMessage};

use base64;
use std::collections::HashMap;

// use google_cloud_storage::http::objects::download::Range;
// use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::upload::UploadObjectRequest;
use google_cloud_storage::sign::SignedURLMethod;
use google_cloud_storage::sign::SignedURLOptions;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use std::str;
use tokio::{sync::Mutex, task::JoinHandle};

use serde_json::{json, Value};

use psst_core::{
    audio::{normalize::NormalizationLevel, output::AudioOutput},
    cache::{Cache, CacheHandle},
    cdn::{Cdn, CdnHandle},
    connection,
    downloader::{item::PlaybackItem, Downloader, PlaybackConfig},
    error::Error,
    item_id::{ItemId, ItemIdType},
    //player::{item::PlaybackItem, PlaybackConfig, Player, PlayerCommand, PlayerEvent},
    session::{SessionConfig, SessionService},
};

use std::{
    convert::TryInto,
    env,
    ffi::OsStr,
    io,
    io::BufRead,
    path::{Path, PathBuf},
    thread,
};

#[derive(Debug, Deserialize)]
struct TracksList {
    tracks_id: Vec<String>,
}

struct Request {
    request_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Message {
    text: String,
}

/// Push Track download request in Pub/Sub
/// Store request in Database to monitor download status
/// Return request UUID
async fn request_tracks_download(
    Extension(app): Extension<Arc<Mutex<AppState>>>,
    Json(input): Json<TracksList>,
) -> impl IntoResponse {
    // let pub_sub_client = PubSubClient::new(
    //     std::env::var("GOOGLE_APPLICATION_CREDENTIALS")
    //         .unwrap()
    //         .as_str(),
    //     Duration::from_secs(30),
    // )
    // .unwrap();

    // let messages = input
    //     .tracks_id
    //     .iter()
    //     .map(|s| base64::encode(json!({ "track_id": s }).to_string()))
    //     .map(|data| RawPublishedMessage::new(data))
    //     .collect::<Vec<_>>();

    // let message_ids = pub_sub_client
    //     .publish_raw(
    //         app.lock().await.config.pubsub_topic.as_str(),
    //         messages,
    //         None,
    //     )
    //     .await
    //     .unwrap();
    // let message_ids = message_ids.join(", ");
    // println!("Published messages with IDs: {message_ids}");

    // Create pubsub client.
    // The default project is determined by credentials.
    // - If the GOOGLE_APPLICATION_CREDENTIALS is specified the project_id is from credentials.
    // - If the server is running on GCP the project_id is from metadata server
    // - If the PUBSUB_EMULATOR_HOST is specified the project_id is 'local-project'
    let mut client = google_cloud_pubsub::client::Client::default()
        .await
        .unwrap();

    // Create topic.
    let topic = client.topic(app.lock().await.config.pubsub_topic.as_str());
    //  if !topic.exists(None, None).await {
    //      topic.create(None, None, None).await;
    //  }

    // Start publisher.
    let mut publisher = topic.new_publisher(None);

    // Publish message.
    //  let tasks : Vec<JoinHandle<Result<String,Status>>> = (0..10).into_iter().map(|_i| {
    //      let publisher = publisher.clone();
    //      tokio::spawn(async move {
    //          let mut msg = PubsubMessage::default();
    //          msg.data = "abc".into();
    //          // Set ordering_key if needed (https://cloud.google.com/pubsub/docs/ordering)
    //          // msg.ordering_key = "order".into();

    //          // Send a message. There are also `publish_bulk` and `publish_immediately` methods.
    //          let mut awaiter = publisher.publish(msg).await;

    //          // The get method blocks until a server-generated ID or an error is returned for the published message.
    //          awaiter.get(None).await
    //      })
    //  }).collect();
    for track in input.tracks_id {
        let mut msg = PubsubMessage::default();
        msg.data = track.into_bytes();
        let mut awaiter = publisher.publish(msg).await;
        let consumer = awaiter.get(None).await;

        //TODO store message id in SQL Table
        dbg!(consumer);
        //dbg!(msg.message_id);
    }

    publisher.shutdown().await;
}

/// A message that is published by publishers and consumed by subscribers. The message must contain either a non-empty data field or at least one attribute. Note that client libraries represent this object differently depending on the language. See the corresponding [client library documentation](https://cloud.google.com/pubsub/docs/reference/libraries) for more information. See [quotas and limits] (https://cloud.google.com/pubsub/quotas) for more information about message limits.
///
/// This type is not used in any activity, and only used as *part* of another schema.
///
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct PubsubMessage1 {
    /// Attributes for this message. If this field is empty, the message must contain non-empty data. This can be used to filter messages on the subscription.
    pub attributes: Option<HashMap<String, String>>,
    /// The message data field. If this field is empty, the message must contain at least one attribute.
    pub data: Option<String>,
    /// ID of this message, assigned by the server when the message is published. Guaranteed to be unique within the topic. This value may be read by a subscriber that receives a `PubsubMessage` via a `Pull` call or a push delivery. It must not be populated by the publisher in a `Publish` call.
    #[serde(rename = "messageId")]
    pub message_id: Option<String>,
    /// If non-empty, identifies related messages for which publish order should be respected. If a `Subscription` has `enable_message_ordering` set to `true`, messages published with the same non-empty `ordering_key` value will be delivered to subscribers in the order in which they are received by the Pub/Sub system. All `PubsubMessage`s published in a given `PublishRequest` must specify the same `ordering_key` value.
    #[serde(rename = "orderingKey")]
    pub ordering_key: Option<String>,
    /// The time at which the message was published, populated by the server when it receives the `Publish` call. It must not be populated by the publisher in a `Publish` call.
    #[serde(rename = "publishTime")]
    pub publish_time: Option<String>,
}

/// Download track from Spotify
/// Store Ogg Vorbis file in Cloud Storage
async fn download_track_from_spotify(
    Extension(app): Extension<Arc<Mutex<AppState>>>,
    Json(msg): Json<PubsubMessage1>,
) {
    let msg = base64::decode(msg.data.unwrap()).unwrap();
    let track_id = str::from_utf8(&msg).unwrap();

    let login_creds = connection::Credentials::from_username_and_password(
        env::var("SPOTIFY_USERNAME").unwrap(),
        env::var("SPOTIFY_PASSWORD").unwrap(),
    );
    dbg!(&login_creds);
    // let track_id = args
    //     .get(1)
    //     .expect("Expected <track_id> in the first parameter");

    let session = SessionService::with_config(SessionConfig {
        login_creds,
        proxy_url: None,
    });

    let cdn = Cdn::new(session.clone(), None).unwrap();
    let cache = Cache::new(PathBuf::from("cache")).unwrap();
    let item_id = ItemId::from_base62(track_id, ItemIdType::Track).unwrap();

    let item = PlaybackItem {
        item_id,
        norm_level: NormalizationLevel::Track,
    };

    let config = PlaybackConfig::default();
    let track = item.load_track(&session, &cache).unwrap();
    let track_name = track.name.unwrap_or_default().to_owned();
    let artist_name = track.artist.get(0).unwrap().name.as_ref().unwrap();
    //let album_name = track.album.unwrap().name.unwrap_or_default().to_owned();

    let track_saved_filename = format!("{} - {}.ogg", track_name, artist_name);
    let track = item.save_as_bytes(&session, cdn, cache, &config);

    // Create client.
    let mut client = google_cloud_storage::client::Client::default()
        .await
        .unwrap();

    // Upload the file
    let uploaded = client
        .upload_object(
            &UploadObjectRequest {
                bucket: app.lock().await.config.cloud_storage_bucket.to_string(),
                name: track_saved_filename,
                ..Default::default()
            },
            &track,
            "application/octet-stream",
            None,
        )
        .await;
}

struct Track {
    id: String,
}

/// Redirect user to Cloud Storage track download link
async fn download_track(
    Extension(app): Extension<Arc<Mutex<AppState>>>,
    Json(input): Json<Track>,
) -> impl IntoResponse {
    // Create client.
    let client = google_cloud_storage::client::Client::default()
        .await
        .unwrap();

    // Create signed url.
    let url_for_download = client
        .signed_url(
            app.lock().await.config.cloud_storage_bucket.as_str(),
            input.id.as_str(),
            SignedURLOptions::default(),
        )
        .await;
    match url_for_download {
        Ok(url) => Redirect::temporary(url.as_str()),
        Err(_) => Redirect::to("/"),
    }
}

async fn get_tracks_from_album(
    spotify: &ClientCredsSpotify,
    album_id: &AlbumId,
) -> Vec<rspotify::model::SimplifiedTrack> {
    dbg!(album_id);
    match spotify
        .album_track_manual(album_id, Some(50), Some(0))
        .await
    {
        Ok(r) => {
            dbg!(&r.items);
            r.items
        }
        Err(_) => vec![],
    }
}

#[derive(Debug, Deserialize)]
struct SearchTracksAlbum {
    album_name: String,
}

/// Return Tracks for a specific Album ID
async fn search_tracks_from_album(
    Extension(app): Extension<Arc<Mutex<AppState>>>,
    Json(input): Json<SearchTracksAlbum>,
) -> impl IntoResponse {
    let result: Vec<rspotify::model::SimplifiedTrack> = vec![];

    // let creds = Credentials::from_env().unwrap();
    // let mut spotify = ClientCredsSpotify::new(creds);
    app.lock()
        .await
        .spotify_client
        .request_token()
        .await
        .unwrap();
    //spotify.request_token().await.unwrap();
    let search = app
        .lock()
        .await
        .spotify_client
        .search(
            &input.album_name,
            &rspotify::model::SearchType::Album,
            None,
            None,
            Some(1),
            None,
        )
        .await;
    match search {
        Ok(r) => {
            dbg!(&r);
            match r {
                rspotify::model::SearchResult::Albums(paged_albums) => {
                    let album = &paged_albums.items.first();
                    match album {
                        Some(album_id) => Json(
                            get_tracks_from_album(
                                &app.lock().await.spotify_client,
                                album_id.id.as_ref().unwrap(),
                            )
                            .await,
                        ),
                        None => Json(vec![]),
                    }
                }
                _ => Json(result),
            }
        }
        Err(_) => Json(result),
    }
}

#[derive(Debug, Clone)]
struct AppConfig {
    cloud_storage_bucket: String,
    pubsub_topic: String,
    postgresql_connection_string: String,
}

struct AppState {
    spotify_client: ClientCredsSpotify,
    config: AppConfig, // PostgreSQL SQLX
}

fn load_config_from_env() -> AppConfig {
    AppConfig {
        cloud_storage_bucket: std::env::var("CLOUD_STORAGE_BUCKET").unwrap(),
        postgresql_connection_string: std::env::var("SUPABASE_URL").unwrap(),
        pubsub_topic: std::env::var("PUBSUB_TOPIC").unwrap(),
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let creds = Credentials::from_env().unwrap();
    let mut spotify = ClientCredsSpotify::new(creds);

    let app_state = AppState {
        config: load_config_from_env(),
        spotify_client: spotify,
    };

    let app_state = Arc::new(Mutex::new(app_state));

    // build our application with a route
    let app = Router::new()
        .route("/search_album", post(search_tracks_from_album))
        .route("/request_tracks_download", post(request_tracks_download))
        .route(
            "/download_track_from_spotify",
            post(download_track_from_spotify),
        )
        .layer(Extension(app_state));

    // run it
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    // dbg!(search_tracks_from_album("Prodigy".to_string()).await);
}
