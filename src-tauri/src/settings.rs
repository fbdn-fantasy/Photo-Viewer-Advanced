use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: String,
    pub default_zoom: String,
    pub autoplay: bool,
    #[serde(rename = "loop")]
    pub loop_mode: String,
    pub slideshow_interval: f64,
    pub show_thumbnails: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self { theme: "system".into(), default_zoom: "fit".into(), autoplay: true,
            loop_mode: "original".into(), slideshow_interval: 5.0, show_thumbnails: false }
    }
}
impl Settings {
    pub fn validate(&mut self) {
        if !["system", "dark", "light"].contains(&self.theme.as_str()) { self.theme = "system".into(); }
        if !["fit", "actual"].contains(&self.default_zoom.as_str()) { self.default_zoom = "fit".into(); }
        if !["original", "forever", "once"].contains(&self.loop_mode.as_str()) { self.loop_mode = "original".into(); }
        if !self.slideshow_interval.is_finite() { self.slideshow_interval = 5.0; }
        self.slideshow_interval = self.slideshow_interval.clamp(1.0, 3600.0);
    }
}

