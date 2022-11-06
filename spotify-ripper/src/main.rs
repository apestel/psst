use axum::{
    body::Body,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::post,
    Extension, Json, Router,
};
use axum_tracing_opentelemetry::opentelemetry_tracing_layer;
use opentelemetry::{
    runtime::Tokio,
    sdk::trace::Sampler,
    trace::{Tracer, TracerProvider as _},
};
use opentelemetry_sdk::trace::TracerProvider;
use opentelemetry_stackdriver::{
    Authorizer, GcpAuthorizer, LogContext, MonitoredResource, StackDriverExporter,
};

use gcp_auth::AuthenticationManager;

use log::{debug, error, info, log_enabled, Level};
use rspotify::{
    model::{AlbumId, Market, SimplifiedTrack},
    prelude::*,
    ClientCredsSpotify, Credentials,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{net::SocketAddr, sync::Arc};
use tracing_subscriber::{prelude::*, Registry};

use google_cloud_googleapis::pubsub::v1::PubsubMessage;

use std::collections::HashMap;

use google_cloud_storage::http::objects::upload::UploadObjectRequest;
//use google_cloud_storage::sign::SignedURLMethod;
use google_cloud_storage::sign::SignedURLOptions;

use std::str;
use tokio::sync::Mutex;

use anyhow;

//use serde_json::{json, Value};

use psst_core::{
    audio::normalize::NormalizationLevel,
    cache::Cache,
    cdn::Cdn,
    connection,
    downloader::{item::PlaybackItem, PlaybackConfig},
    error::Error,
    item_id::{ItemId, ItemIdType},
    //player::{item::PlaybackItem, PlaybackConfig, Player, PlayerCommand, PlayerEvent},
    session::{SessionConfig, SessionService},
};

use std::{convert::TryInto, env, path::PathBuf, thread};

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

enum AppError {
    InternalServerError(anyhow::Error),
    SpotifyError(rspotify::ClientError),
    PsstCoreError(psst_core::error::Error),
    GoogleCloudStorageError(google_cloud_storage::client::Error),
    GoogleCloudPubSubError(google_cloud_pubsub::client::Error),
}

impl From<rspotify::ClientError> for AppError {
    fn from(inner: rspotify::ClientError) -> Self {
        AppError::SpotifyError(inner)
    }
}

impl From<psst_core::error::Error> for AppError {
    fn from(inner: psst_core::error::Error) -> Self {
        AppError::PsstCoreError(inner)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(inner: anyhow::Error) -> Self {
        AppError::InternalServerError(inner)
    }
}

impl From<google_cloud_storage::client::Error> for AppError {
    fn from(inner: google_cloud_storage::client::Error) -> Self {
        AppError::GoogleCloudStorageError(inner)
    }
}

impl From<google_cloud_pubsub::client::Error> for AppError {
    fn from(inner: google_cloud_pubsub::client::Error) -> Self {
        AppError::GoogleCloudPubSubError(inner)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::InternalServerError(inner) => {
                tracing::debug!("stacktrace: {}", inner.backtrace());
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({ "error": "something went wrong"}),
                )
            }
            AppError::SpotifyError(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": format!("spotify connection error: {}", err) }),
            ),
            AppError::GoogleCloudStorageError(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": format!("cloud storage client error: {}", err) }),
            ),
            AppError::GoogleCloudPubSubError(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": format!("Pub/Sub client error: {}", err) }),
            ),
            AppError::PsstCoreError(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": format!("Psst Spotify client error: {}", err) }),
            ),
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}

/// Push Track download request in Pub/Sub
/// Store request in Database to monitor download status
/// Return request UUID
async fn request_tracks_download(
    Extension(app): Extension<Arc<Mutex<AppState>>>,
    Json(input): Json<TracksList>,
) -> Result<impl IntoResponse, AppError> {
    // Create pubsub client.
    // The default project is determined by credentials.
    // - If the GOOGLE_APPLICATION_CREDENTIALS is specified the project_id is from credentials.
    // - If the server is running on GCP the project_id is from metadata server
    // - If the PUBSUB_EMULATOR_HOST is specified the project_id is 'local-project'
    let client = google_cloud_pubsub::client::Client::default().await?;

    // Create topic.
    let topic = client.topic(app.lock().await.config.pubsub_topic.as_str());
    //  if !topic.exists(None, None).await {
    //      topic.create(None, None, None).await;
    //  }

    // Start publisher.
    let mut publisher = topic.new_publisher(None);

    for track in input.tracks_id {
        let msg = PubsubMessage {
            data: track.into_bytes(),
            ..Default::default()
        };

        let awaiter = publisher.publish(msg).await;
        let consumer = awaiter.get(None).await;

        //TODO store message id in SQL Table
        tracing::debug!("Consumer: {:?}", dbg!(&consumer));
    }

    publisher.shutdown().await;
    Ok(())
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

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct PubSubMessageEnvelope {
    pub message: PubsubMessage1,
    pub subscription: String,
}

/// Download track from Spotify
/// Store Ogg Vorbis file in Cloud Storage
async fn download_track_from_spotify(
    Extension(app): Extension<Arc<Mutex<AppState>>>,
    Json(msg): Json<PubSubMessageEnvelope>,
) -> Result<impl IntoResponse, AppError> {
    tracing::debug!("PublishedMessage: {:#?}", &msg);
    if msg.message.data.is_none() {
        return Err(AppError::InternalServerError(anyhow::anyhow!(
            "Pub/Sub message malformed"
        )));
    }
    let msg = match base64::decode(msg.message.data.unwrap()) {
        Ok(msg) => msg,
        Err(_) => {
            return Err(AppError::InternalServerError(anyhow::anyhow!(
                "Pub/Sub message malformed - not base64"
            )))
        }
    };

    let track_id = str::from_utf8(&msg).unwrap();

    let login_creds = connection::Credentials::from_username_and_password(
        env::var("SPOTIFY_USERNAME").unwrap(),
        env::var("SPOTIFY_PASSWORD").unwrap(),
    );

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
    let track = item.load_track(&session, &cache)?;
    let track_name = track.name.unwrap_or_default().to_owned();
    let artist_name = track.artist.get(0).unwrap().name.as_ref().unwrap();
    //let album_name = track.album.unwrap().name.unwrap_or_default().to_owned();

    let track_saved_filename = format!("{} - {}.ogg", track_name, artist_name);
    let track = item.save_as_bytes(&session, cdn, cache, &config)?;

    // Create client.
    let client = google_cloud_storage::client::Client::default().await?;

    // Upload the file
    match client
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
        .await
    {
        Ok(_) => Ok(StatusCode::OK),
        Err(err) => Err(AppError::InternalServerError(anyhow::anyhow!(err))),
    }
}

struct Track {
    id: String,
}

/// Redirect user to Cloud Storage track download link
async fn download_track(
    Extension(app): Extension<Arc<Mutex<AppState>>>,
    Json(input): Json<Track>,
) -> Result<Redirect, AppError> {
    // Create client.
    let client = google_cloud_storage::client::Client::default().await?;

    // Create signed url.
    let url_for_download = client
        .signed_url(
            app.lock().await.config.cloud_storage_bucket.as_str(),
            input.id.as_str(),
            SignedURLOptions::default(),
        )
        .await;
    match url_for_download {
        Ok(url) => Ok(Redirect::to(url.as_str())),
        Err(err) => Err(anyhow::format_err!(err.to_string()).into()),
    }
}

async fn get_tracks_from_album(
    spotify: &ClientCredsSpotify,
    album_id: &AlbumId,
) -> Vec<rspotify::model::SimplifiedTrack> {
    tracing::debug!("Album: {:?}", dbg!(album_id));
    match spotify
        .album_track_manual(album_id, Some(50), Some(0))
        .await
    {
        Ok(r) => {
            tracing::debug!("Tracks: {:#?}", &r.items);
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
) -> Result<axum::Json<Vec<SimplifiedTrack>>, AppError> {
    let result: Vec<rspotify::model::SimplifiedTrack> = vec![];

    // let creds = Credentials::from_env().unwrap();
    // let mut spotify = ClientCredsSpotify::new(creds);
    app.lock().await.spotify_client.request_token().await?;
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
        core::result::Result::Ok(r) => {
            tracing::debug!("Search result: {:?}", dbg!(&r));
            match r {
                rspotify::model::SearchResult::Albums(paged_albums) => {
                    let album = &paged_albums.items.first();
                    match album {
                        Some(album_id) => Ok(Json(
                            get_tracks_from_album(
                                &app.lock().await.spotify_client,
                                album_id.id.as_ref().unwrap(),
                            )
                            .await,
                        )),
                        None => Ok(Json(vec![])),
                    }
                }
                _ => Ok(Json(result)),
            }
        }
        Err(err) => Err(AppError::InternalServerError(anyhow::anyhow!(err))),
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

async fn init_tracing() {
    use axum_tracing_opentelemetry::{
        make_resource,
        otlp,
        //stdio,
    };

    let authentication_manager = AuthenticationManager::new().await.unwrap();
    let project_id = authentication_manager.project_id().await.unwrap();
    let log_context = LogContext {
        log_id: "cloud-trace-test".into(),
        resource: MonitoredResource::GenericNode {
            project_id,
            namespace: Some("test".to_owned()),
            location: None,
            node_id: None,
        },
    };

    let authorizer = GcpAuthorizer::new().await.unwrap();
    let (exporter, driver) = StackDriverExporter::builder()
        .log_context(log_context)
        .build(authorizer)
        .await
        .unwrap();

    tokio::spawn(driver);
    const CLOUD_TRACE_RATE: f64 = 1.0;
    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter.clone(), Tokio)
        .with_config(opentelemetry::sdk::trace::Config {
            sampler: Box::new(Sampler::TraceIdRatioBased(CLOUD_TRACE_RATE)),
            ..Default::default()
        })
        .build();

    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("tracing")));
    // .try_init()
    // .unwrap();

    // let otel_rsrc = make_resource(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    // let otel_tracer = otlp::init_tracer(otel_rsrc, otlp::identity).expect("setup of Tracer");
    // // let otel_tracer =
    // //     stdio::init_tracer(otel_rsrc, stdio::identity, stdio::WriteNoWhere::default())
    // //         .expect("setup of Tracer");
    // let otel_layer = tracing_opentelemetry::layer().with_tracer(otel_tracer);

    // //let subscriber = tracing_subscriber::registry();
    // //.with(otel_layer);
    // let subscriber = Registry::default().with(otel_layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();
}

#[tokio::main]
async fn main() {
    env_logger::init();
    init_tracing().await;
    let creds = Credentials::from_env().unwrap();
    let spotify = ClientCredsSpotify::new(creds);

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
        .layer(Extension(app_state))
        .layer(opentelemetry_tracing_layer());

    // run it
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::warn!("signal received, starting graceful shutdown");
    opentelemetry::global::shutdown_tracer_provider();
}
