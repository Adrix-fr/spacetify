//! Windows version information for `spacetify-service.exe`.

include!("../../build/version_info.rs");

fn main() {
    version_info(
        "spacetify-service",
        "spacetify service - puts Spotify mods back after updates",
    );
}
