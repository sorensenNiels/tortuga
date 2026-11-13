//! Map tiles under the Features tab's map (R-GEO-3, D82): the map asks for the tiles it
//! draws, the add-on fetches them in the background from the tile URL the user set, and
//! keeps the most recently drawn ones as textures.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use egui::TextureHandle;
use tortuga_core::geoserver::map::Tile;
use tortuga_core::{HttpMethod, KeyValue, Request};

use crate::state::AppState;

/// How many tiles are fetched at once.
const AT_ONCE: usize = 6;
/// How many tiles are kept; the least recently drawn go first.
const KEPT: usize = 400;

/// The tiles, shared by the map that draws them and the tasks that fetch them.
pub(crate) type SharedTiles = Arc<Mutex<TileCache>>;

/// The tiles of one tile URL.
#[derive(Default)]
pub(crate) struct TileCache {
    /// The tile URL these tiles are from; another one starts afresh.
    template: String,
    entries: HashMap<Tile, Entry>,
    /// Counts the map's draws, to find the least recently drawn tiles.
    clock: u64,
}

impl std::fmt::Debug for TileCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TileCache")
            .field("template", &self.template)
            .field("tiles", &self.entries.len())
            .finish_non_exhaustive()
    }
}

struct Entry {
    state: State,
    /// The draw that last used it.
    used: u64,
}

enum State {
    /// Drawn, not fetched yet.
    Wanted,
    Loading,
    Ready(TextureHandle),
    /// The server had no image for it; not asked again until the URL changes.
    Failed,
}

/// The cache, also after a task panicked while holding it (it holds no invariant a
/// half-done change could break).
pub(crate) fn lock(tiles: &SharedTiles) -> MutexGuard<'_, TileCache> {
    tiles.lock().unwrap_or_else(PoisonError::into_inner)
}

impl TileCache {
    /// Starts a draw of the map with tiles from `template`.
    pub fn start_draw(&mut self, template: &str) {
        if self.template != template {
            template.clone_into(&mut self.template);
            self.entries.clear();
        }
        self.clock += 1;
    }

    /// Starts a draw of `shown` with tiles from `template`: the ones here, with their
    /// textures; the others are asked for.
    pub fn draw(&mut self, template: &str, shown: &[Tile]) -> Vec<(Tile, TextureHandle)> {
        self.start_draw(template);
        shown
            .iter()
            .filter_map(|tile| self.get(*tile).map(|texture| (*tile, texture)))
            .collect()
    }

    /// The tile's texture, if it is here; otherwise the tile is asked for.
    pub fn get(&mut self, tile: Tile) -> Option<TextureHandle> {
        let clock = self.clock;
        let entry = self.entries.entry(tile).or_insert(Entry {
            state: State::Wanted,
            used: clock,
        });
        entry.used = clock;
        match &entry.state {
            State::Ready(texture) => Some(texture.clone()),
            State::Wanted | State::Loading | State::Failed => None,
        }
    }

    /// The tiles to fetch now, from `template`, nearest the last draw first; they are
    /// marked as loading. Tiles no longer drawn are not fetched.
    fn take_wanted(&mut self, template: &str) -> Vec<Tile> {
        if self.template != template {
            return Vec::new();
        }
        let loading = self
            .entries
            .values()
            .filter(|e| matches!(e.state, State::Loading))
            .count();
        let clock = self.clock;
        let mut wanted: Vec<Tile> = self
            .entries
            .iter()
            .filter(|(_, e)| matches!(e.state, State::Wanted) && e.used == clock)
            .map(|(t, _)| *t)
            .collect();
        wanted.sort();
        wanted.truncate(AT_ONCE.saturating_sub(loading));
        for tile in &wanted {
            if let Some(e) = self.entries.get_mut(tile) {
                e.state = State::Loading;
            }
        }
        // Tiles asked for in an earlier draw and not drawn since are dropped.
        self.entries
            .retain(|_, e| !(matches!(e.state, State::Wanted) && e.used != clock));
        wanted
    }

    /// Takes in a fetched tile; `None` when the server had no image for it.
    fn finished(&mut self, template: &str, tile: Tile, texture: Option<TextureHandle>) {
        if self.template != template {
            return;
        }
        if let Some(e) = self.entries.get_mut(&tile) {
            e.state = texture.map_or(State::Failed, State::Ready);
        }
        self.evict();
    }

    /// Drops the least recently drawn tiles beyond [`KEPT`].
    fn evict(&mut self) {
        if self.entries.len() <= KEPT {
            return;
        }
        let mut done: Vec<(u64, Tile)> = self
            .entries
            .iter()
            .filter(|(_, e)| matches!(e.state, State::Ready(_) | State::Failed))
            .map(|(t, e)| (e.used, *t))
            .collect();
        done.sort();
        let extra = self.entries.len() - KEPT;
        for (_, tile) in done.into_iter().take(extra) {
            self.entries.remove(&tile);
        }
    }
}

/// Fetches the tiles the map asked for (R-GEO-3), from the tile URL in the settings.
pub(crate) fn fetch(state: &AppState, ctx: &egui::Context) {
    let Some(g) = &state.addons.geoserver else {
        return;
    };
    let template = state
        .settings
        .geoserver
        .chosen_basemap()
        .url
        .trim()
        .to_owned();
    let wanted = lock(&g.tiles).take_wanted(&template);
    for tile in wanted {
        let tiles = Arc::clone(&g.tiles);
        let client = state.client.clone();
        let ctx = ctx.clone();
        let template = template.clone();
        state.host.spawn(Box::pin(async move {
            let response = client.execute(&tile_request(&tile.url(&template))).await;
            let texture = response
                .ok()
                .filter(|r| (200..300).contains(&r.status))
                .and_then(|r| crate::components::decode_image(&r))
                .map(|image| {
                    let name = format!("map-tile-{}-{}-{}", tile.z, tile.x, tile.y);
                    let pixels = Arc::unwrap_or_clone(image.pixels);
                    ctx.load_texture(name, pixels, egui::TextureOptions::LINEAR)
                });
            lock(&tiles).finished(&template, tile, texture);
            ctx.request_repaint();
        }));
    }
}

/// A tile's request. Tile servers such as OpenStreetMap's ask clients to let tiles be
/// cached, so the automatic `Cache-Control: no-cache` is replaced.
fn tile_request(url: &str) -> Request {
    let mut request = Request::new(HttpMethod::Get, url);
    request.headers = vec![
        KeyValue::new("Accept", "image/png,image/jpeg,image/webp,image/*"),
        KeyValue::new("Cache-Control", "max-age=86400"),
    ];
    request
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(x: u32) -> Tile {
        Tile { z: 5, x, y: 3 }
    }

    /// R-GEO-3: drawn tiles are asked for, a few at a time; a new tile URL starts afresh.
    #[test]
    fn drawn_tiles_are_fetched_a_few_at_a_time() {
        let mut cache = TileCache::default();
        cache.start_draw("https://a.test/{z}/{x}/{y}.png");
        for x in 0..10 {
            assert!(cache.get(tile(x)).is_none());
        }
        assert_eq!(cache.take_wanted("https://other.test/{z}"), []);
        let first = cache.take_wanted("https://a.test/{z}/{x}/{y}.png");
        assert_eq!(first.len(), AT_ONCE);
        assert_eq!(
            cache.take_wanted("https://a.test/{z}/{x}/{y}.png"),
            [],
            "the rest wait"
        );
        for t in &first {
            cache.finished("https://a.test/{z}/{x}/{y}.png", *t, None);
        }
        // Not drawn again: the rest are no longer wanted.
        cache.start_draw("https://a.test/{z}/{x}/{y}.png");
        assert_eq!(cache.take_wanted("https://a.test/{z}/{x}/{y}.png"), []);
        assert!(cache.get(tile(0)).is_none(), "failed: not asked again");
        assert_eq!(cache.take_wanted("https://a.test/{z}/{x}/{y}.png"), []);

        cache.start_draw("https://b.test/ü😀/{z}/{x}/{y}");
        assert!(cache.get(tile(0)).is_none());
        assert_eq!(
            cache.take_wanted("https://b.test/ü😀/{z}/{x}/{y}"),
            [tile(0)]
        );
    }

    #[test]
    fn tile_requests_let_tiles_be_cached() {
        let r = tile_request("https://a.test/1/2/3.png");
        assert_eq!(r.header("Cache-Control"), Some("max-age=86400"));
        assert_eq!(r.url, "https://a.test/1/2/3.png");
    }
}
