//! Windows version information for `spacetify.exe`.

include!("../../build/version_info.rs");

fn main() {
    version_info("spacetify", "spacetify - mod manager for Spotify");
}
