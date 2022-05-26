pub mod file;
pub mod item;
mod storage;

use std::{mem, thread, thread::JoinHandle, time::Duration};

use crate::{cache::CacheHandle, cdn::CdnHandle, error::Error, session::SessionService};

use self::{
    file::MediaPath,
    item::{LoadedPlaybackItem, PlaybackItem},
};

const PREVIOUS_TRACK_THRESHOLD: Duration = Duration::from_secs(3);
const STOP_AFTER_CONSECUTIVE_LOADING_FAILURES: usize = 3;

#[derive(Clone)]
pub struct PlaybackConfig {
    pub bitrate: usize,
    pub pregain: f32,
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            bitrate: 320,
            pregain: 3.0,
        }
    }
}

pub struct Downloader {
    // state: PlayerState,
    // preload: PreloadState,
    session: SessionService,
    cdn: CdnHandle,
    cache: CacheHandle,
    config: PlaybackConfig,
    // sender: Sender<PlayerEvent>,
    // receiver: Receiver<PlayerEvent>,
    consecutive_loading_failures: usize,
}

impl Downloader {
    pub fn new(
        session: SessionService,
        cdn: CdnHandle,
        cache: CacheHandle,
        config: PlaybackConfig,
    ) -> Self {
        // let (sender, receiver) = unbounded();
        Self {
            session,
            cdn,
            cache,
            config,
            // sender,
            // receiver,
            // state: PlayerState::Stopped,
            // preload: PreloadState::None,
            consecutive_loading_failures: 0,
        }
    }

    // fn handle_loaded(&mut self, item: PlaybackItem, result: Result<LoadedPlaybackItem, Error>) {
    //     match self.state {
    //         PlayerState::Loading {
    //             item: requested_item,
    //             ..
    //         } if item == requested_item => match result {
    //             Ok(loaded_item) => {
    //                 self.consecutive_loading_failures = 0;
    //                 self.play_loaded(loaded_item);
    //             }
    //             Err(err) => {
    //                 self.consecutive_loading_failures += 1;
    //                 if self.consecutive_loading_failures < STOP_AFTER_CONSECUTIVE_LOADING_FAILURES {
    //                     log::error!("skipping, error while loading: {}", err);
    //                     self.next();
    //                 } else {
    //                     log::error!("stopping, error while loading: {}", err);
    //                     self.stop();
    //                 }
    //             }
    //         },
    //         _ => {
    //             log::info!("stale load result received, ignoring");
    //         }
    //     }
    // }

    // fn handle_preloaded(&mut self, item: PlaybackItem, result: Result<LoadedPlaybackItem, Error>) {
    //     match self.preload {
    //         PreloadState::Preloading {
    //             item: requested_item,
    //             ..
    //         } if item == requested_item => match result {
    //             Ok(loaded_item) => {
    //                 log::info!("preloaded audio file");
    //                 self.preload = PreloadState::Preloaded { item, loaded_item };
    //             }
    //             Err(err) => {
    //                 log::error!("failed to preload audio file, error while opening: {}", err);
    //                 self.preload = PreloadState::None;
    //             }
    //         },
    //         _ => {
    //             log::info!("stale preload result received, ignoring");

    //             // We are not preloading this item, but because we sometimes extract the
    //             // preloading thread and use it for loading, let's check if the item is not
    //             // being loaded now.
    //             self.handle_loaded(item, result);
    //         }
    //     }
    // }

    // fn handle_end_of_track(&mut self) {
    //     self.queue.skip_to_following();
    //     if let Some(&item) = self.queue.get_current() {
    //         self.load_and_play(item);
    //     } else {
    //         self.stop();
    //     }
    // }

    // fn load_queue(&mut self, items: Vec<PlaybackItem>, position: usize) {
    //     self.queue.fill(items, position);
    //     if let Some(&item) = self.queue.get_current() {
    //         self.load_and_play(item);
    //     } else {
    //         self.stop();
    //     }
    // }

    // fn load_and_play(&mut self, item: PlaybackItem) {
    //     // Check if the item is already in the preloader state.
    //     let loading_handle = match mem::replace(&mut self.preload, PreloadState::None) {
    //         PreloadState::Preloaded {
    //             item: preloaded_item,
    //             loaded_item,
    //         } if preloaded_item == item => {
    //             // This item is already loaded in the preloader state.
    //             self.play_loaded(loaded_item);
    //             return;
    //         }

    //         PreloadState::Preloading {
    //             item: preloaded_item,
    //             loading_handle,
    //         } if preloaded_item == item => {
    //             // This item is being preloaded. Take it out of the preloader state.
    //             loading_handle
    //         }

    //         preloading_other_file_or_none => {
    //             self.preload = preloading_other_file_or_none;
    //             // Item is not preloaded yet, load it in a background thread.
    //             thread::spawn({
    //                 let sender = self.sender.clone();
    //                 let session = self.session.clone();
    //                 let cdn = self.cdn.clone();
    //                 let cache = self.cache.clone();
    //                 let config = self.config.clone();
    //                 move || {
    //                     let result = item.load(&session, cdn, cache, &config);
    //                     //  sender.send(PlayerEvent::Loaded { item, result }).unwrap();
    //                 }
    //             })
    //         }
    //     };

    //     self.sender.send(PlayerEvent::Loading { item }).unwrap();
    //     self.state = PlayerState::Loading {
    //         item,
    //         _loading_handle: loading_handle,
    //     };
    // }

    // fn preload(&mut self, item: PlaybackItem) {
    //     if self.is_in_preload(item) {
    //         return;
    //     }
    //     let loading_handle = thread::spawn({
    //         let sender = self.sender.clone();
    //         let session = self.session.clone();
    //         let cdn = self.cdn.clone();
    //         let cache = self.cache.clone();
    //         let config = self.config.clone();
    //         move || {
    //             let result = item.load(&session, cdn, cache, &config);
    //             sender
    //                 .send(PlayerEvent::Preloaded { item, result })
    //                 .unwrap();
    //         }
    //     });
    //     self.preload = PreloadState::Preloading {
    //         item,
    //         loading_handle,
    //     };
    // }

    // fn play_loaded(&mut self, loaded_item: LoadedPlaybackItem) {
    //     log::info!("starting playback");
    //     let path = loaded_item.file.path();
    //     let position = Duration::default();
    //     self.playback_mgr.play(loaded_item);
    //     self.state = PlayerState::Playing { path, position };
    //     self.sender
    //         .send(PlayerEvent::Playing { path, position })
    //         .unwrap();
    // }

    // fn pause(&mut self) {
    //     match mem::replace(&mut self.state, PlayerState::Invalid) {
    //         PlayerState::Playing { path, position } | PlayerState::Paused { path, position } => {
    //             log::info!("pausing playback");
    //             self.audio_output_sink.pause();
    //             self.sender
    //                 .send(PlayerEvent::Pausing { path, position })
    //                 .unwrap();
    //             self.state = PlayerState::Paused { path, position };
    //         }
    //         _ => {
    //             log::warn!("invalid state transition");
    //         }
    //     }
    // }

    // fn next(&mut self) {
    //     self.queue.skip_to_next();
    //     if let Some(&item) = self.queue.get_current() {
    //         self.load_and_play(item);
    //     } else {
    //         self.stop();
    //     }
    // }

    // fn configure(&mut self, config: PlaybackConfig) {
    //     self.config = config;
    // }

    // fn is_near_playback_start(&self) -> bool {
    //     match self.state {
    //         PlayerState::Playing { position, .. } | PlayerState::Paused { position, .. } => {
    //             position < PREVIOUS_TRACK_THRESHOLD
    //         }
    //         _ => false,
    //     }
    // }

    // fn is_in_preload(&self, item: PlaybackItem) -> bool {
    //     match self.preload {
    //         PreloadState::Preloading { item: p_item, .. }
    //         | PreloadState::Preloaded { item: p_item, .. } => p_item == item,
    //         _ => false,
    //     }
    // }
}
