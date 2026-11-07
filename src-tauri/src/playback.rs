//! Tauri commands around the Rust queue engine (`player::Queue`).
//! After every change the queue is persisted and a `player-state` event is emitted;
//! when a new track starts we record history, pre-resolve the next stream and top up
//! the queue with radio tracks when it is about to run out.
use crate::models::Track;
use crate::player::{Context, Queue, Snapshot, Step};
use crate::state::Core;
use crate::yt::Codec;
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

type Res<T> = Result<T, String>;
type St<'a> = State<'a, Arc<Core>>;

#[derive(Serialize)]
pub struct StepReply {
    pub step: Step,
    pub state: Snapshot,
}

/// Persist + broadcast; kick off side effects when playback (re)started.
fn publish(app: &AppHandle, core: &Arc<Core>, q: &Queue, started: bool) -> Snapshot {
    let snap = q.snapshot();
    if let Ok(json) = serde_json::to_string(&q.for_storage()) {
        let _ = core.db.set_setting("queue", &json);
    }
    let _ = app.emit("player-state", &snap);
    if started {
        on_started(app, core, q);
    }
    snap
}

fn on_started(app: &AppHandle, core: &Arc<Core>, q: &Queue) {
    let Some(cur) = q.current() else { return };
    let _ = core.db.record_play(&cur.track);
    // Pre-resolve the next stream URL so skipping is instant.
    if let Some(next) = q.index.and_then(|i| q.items.get(i + 1)) {
        let (core, id) = (core.clone(), next.track.id.clone());
        tauri::async_runtime::spawn(async move {
            if core.db.download_path(&id).is_none() {
                let _ = core.yt.stream(&id, Codec::Aac).await;
            }
        });
    }
    // Close to the end with autoplay on: top up with radio in the background.
    let near_end = q.index.is_some_and(|i| i + 2 >= q.items.len());
    if q.autoplay && q.repeat == crate::player::Repeat::Off && near_end {
        let (app, core, seed) = (app.clone(), core.clone(), cur.track.id.clone());
        tauri::async_runtime::spawn(async move {
            if let Ok(tracks) = core.yt.radio(&seed).await {
                let mut q = core.queue.lock().await;
                if q.extend_radio(tracks) > 0 {
                    publish(&app, &core, &q, false);
                }
            }
        });
    }
}

#[tauri::command]
pub async fn player_state(core: St<'_>) -> Res<Snapshot> {
    Ok(core.queue.lock().await.snapshot())
}

#[tauri::command]
pub async fn player_play_list(app: AppHandle, core: St<'_>, tracks: Vec<Track>, start: usize, context: Option<Context>) -> Res<Snapshot> {
    let core = core.inner().clone();
    let mut q = core.queue.lock().await;
    q.play_list(tracks, start, context);
    Ok(publish(&app, &core, &q, true))
}

#[tauri::command]
pub async fn player_play_next(app: AppHandle, core: St<'_>, tracks: Vec<Track>) -> Res<Snapshot> {
    let core = core.inner().clone();
    let mut q = core.queue.lock().await;
    let was_empty = q.index.is_none();
    q.play_next(tracks);
    Ok(publish(&app, &core, &q, was_empty))
}

#[tauri::command]
pub async fn player_add_queue(app: AppHandle, core: St<'_>, tracks: Vec<Track>) -> Res<Snapshot> {
    let core = core.inner().clone();
    let mut q = core.queue.lock().await;
    let was_empty = q.index.is_none();
    q.add_to_queue(tracks);
    Ok(publish(&app, &core, &q, was_empty))
}

#[tauri::command]
pub async fn player_remove(app: AppHandle, core: St<'_>, index: usize) -> Res<Snapshot> {
    let core = core.inner().clone();
    let mut q = core.queue.lock().await;
    q.remove(index);
    Ok(publish(&app, &core, &q, false))
}

#[tauri::command]
pub async fn player_jump(app: AppHandle, core: St<'_>, index: usize) -> Res<Snapshot> {
    let core = core.inner().clone();
    let mut q = core.queue.lock().await;
    q.jump(index);
    Ok(publish(&app, &core, &q, true))
}

/// `auto` = the current track ended by itself (repeat-one applies).
#[tauri::command]
pub async fn player_next(app: AppHandle, core: St<'_>, auto: bool) -> Res<StepReply> {
    let core = core.inner().clone();
    let mut q = core.queue.lock().await;
    if let Some(step) = q.next(auto) {
        let state = publish(&app, &core, &q, step == Step::Play);
        return Ok(StepReply { step, state });
    }
    // End of queue: extend with radio (autoplay) or stop.
    let seed = q.current().map(|c| c.track.id.clone());
    if !(q.autoplay && seed.is_some()) {
        return Ok(StepReply { step: Step::Stop, state: q.snapshot() });
    }
    drop(q);
    let tracks = core.yt.radio(&seed.unwrap_or_default()).await.unwrap_or_default();
    let mut q = core.queue.lock().await;
    if q.extend_radio(tracks) > 0
        && let Some(step) = q.next(false)
    {
        let state = publish(&app, &core, &q, true);
        return Ok(StepReply { step, state });
    }
    Ok(StepReply { step: Step::Stop, state: q.snapshot() })
}

#[tauri::command]
pub async fn player_prev(app: AppHandle, core: St<'_>, position: f64) -> Res<StepReply> {
    let core = core.inner().clone();
    let mut q = core.queue.lock().await;
    let step = q.prev(position);
    let state = publish(&app, &core, &q, step == Step::Play);
    Ok(StepReply { step, state })
}

#[tauri::command]
pub async fn player_toggle_shuffle(app: AppHandle, core: St<'_>) -> Res<Snapshot> {
    let core = core.inner().clone();
    let mut q = core.queue.lock().await;
    q.toggle_shuffle();
    Ok(publish(&app, &core, &q, false))
}

#[tauri::command]
pub async fn player_cycle_repeat(app: AppHandle, core: St<'_>) -> Res<Snapshot> {
    let core = core.inner().clone();
    let mut q = core.queue.lock().await;
    q.cycle_repeat();
    Ok(publish(&app, &core, &q, false))
}

#[tauri::command]
pub async fn player_set_autoplay(app: AppHandle, core: St<'_>, on: bool) -> Res<Snapshot> {
    let core = core.inner().clone();
    let mut q = core.queue.lock().await;
    q.autoplay = on;
    Ok(publish(&app, &core, &q, false))
}
