use std::path::PathBuf;
use std::time::Duration;

use gpui::{Context, WeakEntity};
use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};

use crate::root::EpistolaGui;

const DEBOUNCE: Duration = Duration::from_millis(200);

/// Owns the OS-level watch; dropping it (e.g. on collection switch) stops it.
pub struct FsWatcher {
    _watcher: RecommendedWatcher,
}

/// Watches `root` recursively and delivers debounced batches of touched paths
/// to `EpistolaGui::handle_fs_events`. Returns `None` if the OS watch couldn't
/// be established (e.g. the path is gone) — callers just end up unwatched,
/// same as never having called this.
pub fn spawn_watch(root: PathBuf, cx: &mut Context<EpistolaGui>) -> Option<FsWatcher> {
    let (tx, rx) = async_channel::unbounded::<PathBuf>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        for path in event.paths {
            let _ = tx.send_blocking(path);
        }
    })
    .ok()?;
    watcher.watch(&root, RecursiveMode::Recursive).ok()?;

    cx.spawn(async move |weak: WeakEntity<EpistolaGui>, cx| {
        while let Ok(first) = rx.recv().await {
            cx.background_executor().timer(DEBOUNCE).await;
            let mut paths = vec![first];
            while let Ok(path) = rx.try_recv() {
                paths.push(path);
            }
            if weak
                .update(cx, |gui, cx| gui.handle_fs_events(paths, cx))
                .is_err()
            {
                break;
            }
        }
    })
    .detach();

    Some(FsWatcher { _watcher: watcher })
}
