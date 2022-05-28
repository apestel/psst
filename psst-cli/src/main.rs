use psst_core::{
    audio::{normalize::NormalizationLevel, output::AudioOutput},
    cache::{Cache, CacheHandle},
    cdn::{Cdn, CdnHandle},
    connection::Credentials,
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

fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    let login_creds = Credentials::from_username_and_password(
        env::var("SPOTIFY_USERNAME").unwrap(),
        env::var("SPOTIFY_PASSWORD").unwrap(),
    );
    dbg!(&login_creds);
    let track_id = args
        .get(1)
        .expect("Expected <track_id> in the first parameter");

    let session = SessionService::with_config(SessionConfig {
        login_creds,
        proxy_url: None,
    });
    start(track_id, session).unwrap();
}

fn start(track_id: &str, session: SessionService) -> Result<(), Error> {
    let cdn = Cdn::new(session.clone(), None)?;
    let cache = Cache::new(PathBuf::from("cache"))?;
    let item_id = ItemId::from_base62(track_id, ItemIdType::Track).unwrap();
    save_item(
        session,
        cdn,
        cache,
        PlaybackItem {
            item_id,
            norm_level: NormalizationLevel::Track,
        },
    )
}

fn save_item(
    session: SessionService,
    cdn: CdnHandle,
    cache: CacheHandle,
    item: PlaybackItem,
) -> Result<(), Error> {
    //let output = AudioOutput::open()?;
    let config = PlaybackConfig::default();
    let track = item.load_track(&session, &cache)?;
    let track_name = track.name.unwrap_or_default().to_owned();
    let artist_name = track.artist.get(0).unwrap().name.as_ref().unwrap();
    //let album_name = track.album.unwrap().name.unwrap_or_default().to_owned();

    let track_saved_filename = format!("{} - {}.ogg", track_name, artist_name);
    item.save(
        &session,
        cdn,
        cache,
        &config,
        Path::new(&track_saved_filename),
    )
    .unwrap();
    Ok(())
}
