//! Layout dimensions shared across views: panel sizes, row heights, artwork sizes.

/// Initial and minimum window size.
pub const WINDOW_SIZE: [f32; 2] = [1180.0, 760.0];
pub const MIN_WINDOW_SIZE: [f32; 2] = [760.0, 480.0];

/// Room for the macOS traffic lights over the full-size content view.
pub const TOP_INSET: f32 = if cfg!(target_os = "macos") { 34.0 } else { 14.0 };

// Mini player
pub const MINI_PLAYER_ART: f32 = 56.0;
/// Space above the artwork (clears the macOS traffic lights).
pub const MINI_PLAYER_INSET: f32 = if cfg!(target_os = "macos") { 30.0 } else { 14.0 };
/// Artwork row, seek bar and bottom padding.
pub const MINI_PLAYER_SIZE: [f32; 2] = [400.0, MINI_PLAYER_INSET + MINI_PLAYER_ART + 36.0];

// Panels
pub const SIDEBAR_WIDTH: f32 = 248.0;
pub const QUEUE_WIDTH: f32 = 340.0;
pub const PLAYER_BAR_HEIGHT: f32 = 92.0;
/// Horizontal padding of the main content area.
pub const CONTENT_MARGIN: i8 = 36;
pub const SEARCH_FIELD: [f32; 2] = [360.0, 38.0];

// Rows
pub const TRACK_ROW: f32 = 56.0;
pub const QUEUE_ROW: f32 = 52.0;
pub const SIDEBAR_PLAYLIST_ROW: f32 = 48.0;
pub const NAV_ROW: f32 = 36.0;
pub const ACCOUNT_CARD: f32 = 52.0;

// Artwork (points on screen)
pub const TRACK_ART: f32 = 40.0;
pub const LIST_ART: f32 = 36.0;
pub const NOW_PLAYING_ART: f32 = 56.0;
pub const HEADER_ART: f32 = 200.0;
pub const HOME_CARD: f32 = 168.0;
/// Title and subtitle under a card's artwork.
pub const CARD_TEXT: f32 = 50.0;
pub const GRID_CARD: f32 = 176.0;
pub const FLOW_TILE: f32 = 116.0;
pub const LOGO: f32 = 112.0;

// Artwork (pixels requested from the CDN: ~2x the on-screen size for Retina)
pub const THUMB_PX: u32 = 80;
pub const NOW_PLAYING_PX: u32 = 112;
pub const HEADER_PX: u32 = 400;
pub const HOME_CARD_PX: u32 = 336;
pub const GRID_CARD_PX: u32 = 352;
pub const FLOW_TILE_PX: u32 = 240;

/// Corner radii.
pub mod radius {
    /// Small thumbnails.
    pub const THUMB: u8 = 4;
    /// Hoverable list rows.
    pub const ROW: u8 = 6;
    /// Artwork cards, collection headers.
    pub const CARD: u8 = 8;
    /// Menu rows, selected nav items.
    pub const CONTROL: u8 = 8;
    /// Cards in the sidebar.
    pub const PANEL: u8 = 10;
    /// Popover menus.
    pub const POPOVER: u8 = 12;
}
