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

use std::{env, io, io::BufRead, path::PathBuf, thread};
use symphonia::core::{
    audio::{SampleBuffer, SignalSpec},
    conv::IntoSample,
    units::TimeBase,
};

fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let track_id = args
        .get(1)
        .expect("Expected <track_id> in the first parameter");
    let login_creds = Credentials::from_username_and_password(
        env::var("SPOTIFY_USERNAME").unwrap(),
        env::var("SPOTIFY_PASSWORD").unwrap(),
    );
    dbg!(&login_creds);

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
    play_item(
        session,
        cdn,
        cache,
        PlaybackItem {
            item_id,
            norm_level: NormalizationLevel::Track,
        },
    )
}

fn play_item(
    session: SessionService,
    cdn: CdnHandle,
    cache: CacheHandle,
    item: PlaybackItem,
) -> Result<(), Error> {
    //let output = AudioOutput::open()?;
    let config = PlaybackConfig::default();

    //let mut downloader = Downloader::new(session, cdn, cache, config);
    //item.load_track(&session, cdn, cache, &config);
    //item.load_track(&session, &cache);
    let mut pb = item.load(&session, cdn, cache, &config).unwrap();
    let mut input_packet: SampleBuffer<f32>;
    const DEFAULT_MAX_FRAMES: u64 = 8 * 1024;

    let max_input_frames = pb
        .source
        .codec_params()
        .max_frames_per_packet
        .unwrap_or(DEFAULT_MAX_FRAMES);
    println!("Max Input Frames: {}", max_input_frames);
    //dbg!(pb.source.codec_params());
    dbg!(pb.source.signal_spec());
    input_packet = SampleBuffer::new(max_input_frames, pb.source.signal_spec());

    match pb.source.read_packet(&mut input_packet) {
        Some(_) => println!("Reading packet!!! {}", input_packet.samples().len()),
        None => println!("No packet to read :("),
    }
    dbg!(input_packet.is_empty());

    // match self.input.read_packet(&mut self.input_packet) {
    //     Some(_) => {
    //         self.samples_to_write = 0..self.input_packet.samples().len();
    //         self.is_reading = true;
    //         self.this.send(Msg::Read)?;
    //     }
    //     None => {
    //         self.is_reading = false;
    //         self.total_samples
    //             .store(self.samples_written, Ordering::Relaxed);
    //     }
    // }

    // let mut player = Player::new(session, cdn, cache, config, &output);

    // let _ui_thread = thread::spawn({
    //     let player_sender = player.sender();

    //     player_sender
    //         .send(PlayerEvent::Command(PlayerCommand::LoadQueue {
    //             items: vec![item, item, item],
    //             position: 0,
    //         }))
    //         .unwrap();

    //     move || {
    //         for line in io::stdin().lock().lines() {
    //             match line.as_ref().map(|s| s.as_str()) {
    //                 Ok("p") => {
    //                     player_sender
    //                         .send(PlayerEvent::Command(PlayerCommand::Pause))
    //                         .unwrap();
    //                 }
    //                 Ok("r") => {
    //                     player_sender
    //                         .send(PlayerEvent::Command(PlayerCommand::Resume))
    //                         .unwrap();
    //                 }
    //                 Ok("s") => {
    //                     player_sender
    //                         .send(PlayerEvent::Command(PlayerCommand::Stop))
    //                         .unwrap();
    //                 }
    //                 Ok("<") => {
    //                     player_sender
    //                         .send(PlayerEvent::Command(PlayerCommand::Previous))
    //                         .unwrap();
    //                 }
    //                 Ok(">") => {
    //                     player_sender
    //                         .send(PlayerEvent::Command(PlayerCommand::Next))
    //                         .unwrap();
    //                 }
    //                 _ => log::warn!("unknown command"),
    //             }
    //         }
    //     }
    // });

    // for event in player.receiver() {
    //     player.handle(event);
    // }
    //  output.sink().close();

    Ok(())
}
