//! omarchy-screensaver-starry -- a starry-night screensaver for Omarchy.
//!
//! Placeholder binary for the next branch: a twinkling starfield that gathers
//! into whatever `omarchy branding screensaver` has set, using the shared
//! stage machinery in `omarchy-screensaver-core`. Not yet implemented.

use omarchy_screensaver_core::{sayln, warn};

fn main() {
    warn(format_args!(
        "omarchy-screensaver-starry: not yet implemented -- see the feature/starry-gather branch"
    ));
    let _ = sayln!("starry: coming soon");
    std::process::exit(2);
}
