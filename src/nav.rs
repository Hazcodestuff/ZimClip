//! Sidebar layout state and geometry. Pure, so it is tested on Linux.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Left navigation rail width in the Sidebar layout (Polished only).
pub const SB: i32 = 156;
const HOME_W: i32 = 520;

static SIDEBAR_ON: AtomicBool = AtomicBool::new(false);
static RECOVER: AtomicBool = AtomicBool::new(false);
static NAV_PAGE: AtomicUsize = AtomicUsize::new(0);

pub fn sidebar() -> bool {
    SIDEBAR_ON.load(Ordering::Relaxed)
}
pub fn set_sidebar(on: bool) {
    SIDEBAR_ON.store(on, Ordering::Relaxed)
}
/// 0 = Home, 1 = Clips.
pub fn nav_page() -> usize {
    NAV_PAGE.load(Ordering::Relaxed)
}
pub fn set_nav_page(p: usize) {
    NAV_PAGE.store(p.min(1), Ordering::Relaxed);
    // Any page, UI or layout change starts on the Capture sub-page.
    HOME_SUB.store(0, Ordering::Relaxed);
}
/// Home sub-page when the Sidebar Home column is split because it does not fit: 0 = Capture,
/// 1 = Sources. Independent of the Home/Clips page.
static HOME_SUB: AtomicUsize = AtomicUsize::new(0);
pub fn home_sub() -> usize {
    HOME_SUB.load(Ordering::Relaxed)
}
/// Effective sub-page: the Sources page exists only while the Sources card is not hidden.
pub fn home_sub_for(c: &HomeCards) -> usize {
    if c.is_hidden(Card::Sources) { 0 } else { home_sub() }
}
pub fn set_home_sub(p: usize) {
    HOME_SUB.store(p.min(1), Ordering::Relaxed)
}

/// Compact and Stacked layout page when paging applies (layouts 1 and 3): 0 = Home, 1 = Clips.
static STACKED_SHORT: AtomicBool = AtomicBool::new(false);
/// Set by the layout from the client height: Legacy Stacked pages only when it is too short to
/// show Home and the library side by side.
pub fn set_stacked_short(short: bool) {
    STACKED_SHORT.store(short, Ordering::Relaxed)
}
pub fn compact_page(layout: usize, polished: bool, settings: bool) -> Option<usize> {
    // Polished pages Compact and Stacked. Legacy pages Compact only: its library cannot share the
    // 610 px column with Home. Legacy Stacked keeps both side by side (short heights are stage B).
    (!settings && ((polished && (layout == 1 || layout == 3)) || (!polished && (layout == 1 || (layout == 3 && STACKED_SHORT.load(Ordering::Relaxed)))))).then(nav_page)
}

/// Whether a recording is waiting to be recovered. The Recover button (id 44) is shown
/// on the Home page only when this is true.
pub fn recover_pending() -> bool {
    RECOVER.load(Ordering::Relaxed)
}
pub fn set_recover(p: bool) {
    RECOVER.store(p, Ordering::Relaxed)
}

/// Single rule for the Recover button (id 44): shown only while a recording is pending, Settings
/// is closed, and, in a paged layout, only on the Home page. Clips never shows it.
pub fn recover_visible(pending: bool, settings_open: bool, paged: bool, page: usize) -> bool {
    pending && !settings_open && (!paged || visible(page).home)
}

/// Home page controls to force visible, as (id, visible). Recover follows the real state.
pub fn home_buttons(recover_pending: bool) -> [(i32, bool); 6] {
    // 114 is the Arrange button; it is pinned so hidden cards can always be brought back.
    [(1, true), (10, true), (43, true), (40, true), (44, recover_pending), (114, true)]
}

/// Home visibility for a split Sidebar Home: `sub` is None when not split.
pub fn home_show_sub(expanded: bool, c: &HomeCards, sub: Option<usize>) -> HomeShow {
    let mut s = home_show(expanded, c);
    match sub {
        Some(0) => s.sources = false,
        Some(_) => {
            s.buffer = false;
            s.save = false;
        }
        None => {}
    }
    s.hint = s.hint && sub.is_none();
    s
}

/// Whether the capture status text (warning and encoder rows) may be updated on screen.
pub fn home_text_allowed(sidebar: bool, page: usize) -> bool {
    !sidebar || visible(page).home
}

/// Cards on the Home page that can be arranged. Save clip is essential and cannot be hidden.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Card {
    Buffer,
    Save,
    Sources,
}
impl Card {
    pub const ALL: [Card; 3] = [Card::Buffer, Card::Save, Card::Sources];
    pub fn name(self) -> &'static str {
        match self {
            Card::Buffer => "Buffer",
            Card::Save => "Save clip",
            Card::Sources => "Audio sources",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Card::Buffer => "buffer",
            Card::Save => "save",
            Card::Sources => "sources",
        }
    }
    fn parse(s: &str) -> Option<Card> {
        Card::ALL.into_iter().find(|c| c.key() == s.trim())
    }
    /// Height of the card in pixels, from the Balanced geometry.
    pub fn height(self) -> i32 {
        match self {
            Card::Buffer => 44,
            Card::Save => 52,
            Card::Sources => 224,
        }
    }
    pub fn essential(self) -> bool {
        self == Card::Save
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct HomeCards {
    pub order: [Card; 3],
    pub hidden: [bool; 3], // indexed like Card::ALL
}
impl Default for HomeCards {
    fn default() -> Self {
        HomeCards { order: Card::ALL, hidden: [false; 3] }
    }
}
fn slot_of(c: Card) -> usize {
    Card::ALL.iter().position(|x| *x == c).unwrap()
}
impl HomeCards {
    pub fn is_hidden(&self, c: Card) -> bool {
        self.hidden[slot_of(c)] && !c.essential()
    }
    /// Hide or show a card. Returns false for an essential card, which is refused.
    pub fn toggle_hidden(&mut self, c: Card) -> bool {
        if c.essential() {
            return false;
        }
        let i = slot_of(c);
        self.hidden[i] = !self.hidden[i];
        true
    }
    /// Move a card one place up (dir < 0) or down. Returns false at the edge.
    pub fn shift(&mut self, c: Card, dir: i32) -> bool {
        let i = self.order.iter().position(|x| *x == c).unwrap() as i32;
        let j = i + dir.signum();
        if j < 0 || j > 2 || dir == 0 {
            return false;
        }
        self.order.swap(i as usize, j as usize);
        true
    }
    /// Visible cards with their y, stacked from `top` with an 8 px gap.
    pub fn flow(&self, top: i32) -> Vec<(Card, i32)> {
        let mut y = top;
        let mut out = vec![];
        for c in self.order {
            if self.is_hidden(c) {
                continue;
            }
            out.push((c, y));
            y += c.height() + 8;
        }
        out
    }
    pub fn encode(&self) -> String {
        let order: Vec<&str> = self.order.iter().map(|c| c.key()).collect();
        let hidden: Vec<&str> = Card::ALL.iter().filter(|c| self.is_hidden(**c)).map(|c| c.key()).collect();
        format!("order={}\nhidden={}\n", order.join(","), hidden.join(","))
    }
    /// Lenient parse: anything unknown, duplicated or missing falls back to the default
    /// order for that part. Save clip is never hidden.
    pub fn parse(text: &str) -> HomeCards {
        let mut out = HomeCards::default();
        for line in text.lines().take(8) {
            let Some((k, v)) = line.split_once('=') else { continue };
            let cards: Vec<Card> = v.split(',').filter_map(Card::parse).collect();
            match k.trim() {
                "order" => {
                    let mut seen: Vec<Card> = vec![];
                    for c in cards {
                        if !seen.contains(&c) {
                            seen.push(c);
                        }
                    }
                    if seen.len() == 3 {
                        out.order = [seen[0], seen[1], seen[2]];
                    }
                }
                "hidden" => {
                    for c in cards {
                        if !c.essential() {
                            out.hidden[slot_of(c)] = true;
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }
}

static CARDS: std::sync::Mutex<HomeCards> = std::sync::Mutex::new(HomeCards { order: Card::ALL, hidden: [false; 3] });
pub fn cards() -> HomeCards {
    *CARDS.lock().unwrap()
}
pub fn set_cards(c: HomeCards) {
    *CARDS.lock().unwrap() = c;
}
/// Load the saved arrangement. A missing or unreadable file means the default layout.
pub fn load_cards(root: &std::path::Path) {
    let text = std::fs::read_to_string(root.join("home-cards.ini")).unwrap_or_default();
    if text.len() <= 2048 {
        set_cards(HomeCards::parse(&text));
    }
}
/// Save atomically (temp file, then rename), so a failed write leaves the old file intact.
pub fn save_cards(root: &std::path::Path, c: &HomeCards) -> Result<(), String> {
    let tmp = root.join("home-cards.ini.tmp");
    std::fs::write(&tmp, c.encode()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, root.join("home-cards.ini")).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

/// What the Home page shows, from Details state and the card arrangement.
#[derive(Debug, PartialEq, Eq)]
pub struct HomeShow {
    pub hint: bool,
    pub details: bool,
    pub buffer: bool,
    pub sources: bool,
    pub save: bool,
}
pub fn home_show(expanded: bool, c: &HomeCards) -> HomeShow {
    HomeShow {
        hint: !expanded,
        details: expanded,
        buffer: !expanded && !c.is_hidden(Card::Buffer),
        sources: !expanded && !c.is_hidden(Card::Sources),
        save: true,
    }
}

/// Owner-drawn menu items are dimmed when the state has ODS_DISABLED (0x4) or ODS_GRAYED (0x2).
/// MF_GRAYED items (the Arrange Home end-of-list moves) arrive as ODS_GRAYED only.
pub fn item_dimmed(state: u32) -> bool {
    state & 0x0004 != 0 || state & 0x0002 != 0
}

/// x and width of the Home capture column, centered right of the rail.
pub fn home_col(client_right: i32) -> (i32, i32) {
    if home_wide(client_right) {
        return (SB + 40, HOME_W_NARROW);
    }
    (SB + ((client_right - SB - HOME_W) / 2).max(16), HOME_W)
}
const HOME_W_NARROW: i32 = 440;
/// Wide windows show the capture column on the left and the real-state panel on the right.
pub fn home_wide(client_right: i32) -> bool {
    client_right >= 1000
}
/// Narrow panels stack the three stat cards as rows instead of squeezing three columns.
pub fn panel_stack(width: i32) -> bool {
    width < 330
}
/// Height of the stat area, and the y where the Recent clips heading sits.
pub fn panel_heading_y(width: i32) -> i32 {
    if panel_stack(width) { 104 + 3 * 42 + 14 } else { 104 + 88 + 20 }
}
/// x and width of the Home real-state panel (right of the capture column).
pub fn home_panel(client_right: i32) -> (i32, i32) {
    let x = SB + 40 + HOME_W_NARROW + 32;
    (x, (client_right - 24 - 20 - x).max(200))
}
/// x and width of the Clips library column.
pub fn library_col(client_right: i32) -> (i32, i32) {
    (SB + 34, (client_right - SB - 68).max(300))
}

/// Which page's controls are visible. Exactly one is, for any page value.
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Visible {
    pub home: bool,
    pub library: bool,
}
pub fn visible(page: usize) -> Visible {
    let clips = page == 1;
    Visible { home: !clips, library: clips }
}
/// Home page extras: (simple hint, details+audio, source rows).
pub fn home_detail(expanded: bool) -> (bool, bool, bool) {
    (!expanded, expanded, !expanded)
}


/// Overflow indicator geometry for a page of `content` px shown in `view` px at scroll `offset`:
/// (top, size) of the thumb inside an 8 px margin, or None when everything fits.
pub fn scroll_thumb(view: i32, content: i32, offset: i32) -> Option<(i32, i32)> {
    if view >= content || view < 80 {
        return None;
    }
    let track = view - 16;
    let size = (track * view / content).max(36).min(track);
    let top = 8 + (track - size) * offset.clamp(0, content - view) / (content - view);
    Some((top, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exactly_one_page_is_visible() {
        for page in 0..4 {
            let v = visible(page);
            assert!(v.home != v.library, "page {page}");
        }
        assert_eq!(visible(0), Visible { home: true, library: false });
        assert_eq!(visible(1), Visible { home: false, library: true });
    }
    #[test]
    fn paging_in_polished_compact_stacked_and_legacy_compact_outside_settings() {
        set_nav_page(1);
        assert_eq!(compact_page(1, true, false), Some(1));
        assert_eq!(compact_page(1, false, false), Some(1));
        assert_eq!(compact_page(3, false, false), None);
        assert_eq!(compact_page(1, false, true), None);
        assert_eq!(compact_page(1, true, true), None);
        assert_eq!(compact_page(0, true, false), None);
        assert_eq!(compact_page(3, true, false), Some(1));
        assert_eq!(compact_page(2, true, false), None);
        assert_eq!(compact_page(4, true, false), None);
        set_nav_page(0);
        assert_eq!(compact_page(1, true, false), Some(0));
    }
    #[test]
    fn page_is_clamped() {
        set_nav_page(7);
        assert_eq!(nav_page(), 1);
        set_nav_page(0);
        assert_eq!(nav_page(), 0);
    }
    #[test]
    fn columns_clear_the_rail_and_fit_the_window() {
        for right in [1104, 1200, 1500, 1920] {
            let (x, w) = home_col(right);
            assert!(x >= SB + 16 && x + w <= right - 24, "home {right}");
            let (lx, lw) = library_col(right);
            assert!(lx >= SB + 16 && lx + lw <= right - 24, "library {right}");
        }
        for right in [1000, 1104, 1500] {
            let (px, pw) = home_panel(right);
            let (hx, hw) = home_col(right);
            if home_wide(right) {
                assert!(hx + hw < px && px + pw <= right - 24, "panel {right}");
            }
        }
        // Minimum client width of the polished window (1104): both fit with margin.
        assert!(library_col(1104).1 >= 300);
    }
    #[test]
    fn recover_visible_follows_pending_page_and_settings() {
        // Unpaged layouts: pending decides, page is irrelevant.
        assert!(recover_visible(true, false, false, 1));
        assert!(!recover_visible(false, false, false, 0));
        // Paged layouts: Home only.
        assert!(recover_visible(true, false, true, 0));
        assert!(!recover_visible(true, false, true, 1));
        // Relayout back to Home while still pending shows it again; settings always hides it.
        assert!(recover_visible(true, false, true, 0));
        assert!(!recover_visible(true, true, true, 0));
        assert!(!recover_visible(true, true, false, 0));
    }
    #[test]
    fn recover_button_follows_pending_state() {
        let on = home_buttons(true);
        let off = home_buttons(false);
        assert!(on.iter().find(|b| b.0 == 44).unwrap().1);
        assert!(!off.iter().find(|b| b.0 == 44).unwrap().1);
        assert!(off.iter().filter(|b| b.0 != 44).all(|b| b.1));
        set_recover(true);
        assert!(recover_pending());
        set_recover(false);
        assert!(!recover_pending());
    }
    #[test]
    fn status_text_hidden_on_clips_only_in_sidebar() {
        assert!(home_text_allowed(true, 0));
        assert!(!home_text_allowed(true, 1));
        // Legacy and the four split layouts are unaffected by the page value.
        assert!(home_text_allowed(false, 1));
        assert!(home_text_allowed(false, 0));
    }
    #[test]
    fn save_card_cannot_be_hidden() {
        let mut c = HomeCards::default();
        assert!(!c.toggle_hidden(Card::Save));
        assert!(c.flow(230).iter().any(|(k, _)| *k == Card::Save));
        // A hand-edited file cannot hide it either.
        let p = HomeCards::parse("hidden=save,sources\n");
        assert!(!p.is_hidden(Card::Save) && p.is_hidden(Card::Sources));
    }
    #[test]
    fn flow_stacks_visible_cards_without_overlap() {
        let mut c = HomeCards::default();
        let f = c.flow(230);
        assert_eq!(f.iter().map(|x| x.0).collect::<Vec<_>>(), vec![Card::Buffer, Card::Save, Card::Sources]);
        for w in f.windows(2) {
            assert!(w[0].1 + w[0].0.height() + 8 <= w[1].1);
        }
        // Default fits the original column: last card ends at 566.
        assert_eq!(f[2].1 + Card::Sources.height(), 230 + 44 + 8 + 52 + 8 + 224);
        c.toggle_hidden(Card::Sources);
        c.toggle_hidden(Card::Buffer);
        assert_eq!(c.flow(230), vec![(Card::Save, 230)]);
    }
    #[test]
    fn reorder_and_reset() {
        let mut c = HomeCards::default();
        assert!(!c.shift(Card::Buffer, -1));
        assert!(c.shift(Card::Sources, -1));
        assert_eq!(c.order, [Card::Buffer, Card::Sources, Card::Save]);
        assert!(!c.shift(Card::Save, 1));
        c = HomeCards::default();
        assert_eq!(c, HomeCards::parse(""));
    }
    #[test]
    fn encode_parse_roundtrip_and_bad_input() {
        let mut c = HomeCards::default();
        c.shift(Card::Sources, -1);
        c.shift(Card::Sources, -1);
        c.toggle_hidden(Card::Buffer);
        assert_eq!(HomeCards::parse(&c.encode()), c);
        // Duplicates, unknown names and missing entries fall back to the default order.
        assert_eq!(HomeCards::parse("order=save,save,buffer\n").order, Card::ALL);
        assert_eq!(HomeCards::parse("order=a,b,c\nhidden=zzz\n"), HomeCards::default());
        assert_eq!(HomeCards::parse("\u{0}\u{1}garbage"), HomeCards::default());
    }
    #[test]
    fn save_failure_keeps_previous_file() {
        let dir = std::env::temp_dir().join(format!("zimclip-nav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut c = HomeCards::default();
        c.toggle_hidden(Card::Sources);
        save_cards(&dir, &c).unwrap();
        // Make the destination unwritable by turning it into a directory: rename must fail.
        let bad = std::env::temp_dir().join(format!("zimclip-nav-bad-{}", std::process::id()));
        std::fs::create_dir_all(bad.join("home-cards.ini")).unwrap();
        assert!(save_cards(&bad, &HomeCards::default()).is_err());
        assert!(!bad.join("home-cards.ini.tmp").exists());
        let back = std::fs::read_to_string(dir.join("home-cards.ini")).unwrap();
        assert_eq!(HomeCards::parse(&back), c);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&bad);
    }
    #[test]
    fn home_show_respects_details_and_cards() {
        let mut c = HomeCards::default();
        let s = home_show(false, &c);
        assert!(s.buffer && s.sources && s.hint && !s.details && s.save);
        c.toggle_hidden(Card::Sources);
        assert!(!home_show(false, &c).sources);
        let d = home_show(true, &HomeCards::default());
        assert!(!d.buffer && !d.sources && !d.hint && d.details && d.save);
    }
    #[test]
    fn menu_items_dim_for_disabled_or_grayed_only() {
        assert!(item_dimmed(0x4));
        assert!(item_dimmed(0x2));
        assert!(item_dimmed(0x6));
        assert!(!item_dimmed(0));
        assert!(!item_dimmed(0x1)); // ODS_SELECTED
        assert!(!item_dimmed(0x40)); // ODS_HOTLIGHT / other bits
    }
    #[test]
    fn details_swaps_hint_for_details() {
        assert_eq!(home_detail(false), (true, false, true));
        assert_eq!(home_detail(true), (false, true, false));
    }

    #[test]
    fn scroll_thumb_follows_offset_and_hides_when_content_fits() {
        assert_eq!(scroll_thumb(900, 862, 0), None);
        assert_eq!(scroll_thumb(40, 862, 0), None);
        let (top0, size) = scroll_thumb(700, 862, 0).unwrap();
        assert_eq!(top0, 8);
        assert!(size >= 36 && size < 684);
        let (top_end, size_end) = scroll_thumb(700, 862, 162).unwrap();
        assert_eq!(size, size_end);
        assert_eq!(top_end + size_end, 8 + 684);
        // Out-of-range offsets are clamped.
        assert_eq!(scroll_thumb(700, 862, 9999), scroll_thumb(700, 862, 162));
    }
}
