use std::process::Command;
use std::sync::LazyLock;

static FONT_CACHE: LazyLock<Vec<String>> = LazyLock::new(load_fonts);

#[cfg(target_os = "linux")]
fn load_fonts() -> Vec<String> {
    let fc_list_result = Command::new("fc-list").arg(":").arg("family").output();

    let mut fonts: Vec<_> = match fc_list_result {
        Ok(output) => String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .collect(),
        Err(error) => {
            tracing::warn!("could not load all font families: {error}");
            tracing::warn!("ensure fontconfig is installed (e.g. `sudo apt install fontconfig`)");
            Default::default()
        }
    };

    fonts.sort();
    fonts
}

#[cfg(target_os = "macos")]
fn load_fonts() -> Vec<String> {
    // Try fc-list first (fast, available via Homebrew fontconfig)
    if let Ok(output) = Command::new("fc-list").arg(":").arg("family").output()
        && output.status.success()
    {
        let mut fonts: Vec<_> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        fonts.sort();
        return fonts;
    }

    // Fallback: system_profiler (built-in, slower)
    tracing::debug!(
        "fc-list not found, falling back to system_profiler (install fontconfig via Homebrew for faster font loading)"
    );
    let result = Command::new("system_profiler")
        .arg("SPFontsDataType")
        .output();

    let mut fonts: Vec<_> = match result {
        Ok(output) => String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|l| l.trim().strip_prefix("Family: ").map(str::to_string))
            .collect(),
        Err(error) => {
            tracing::warn!("could not load all font families: {error}");
            Default::default()
        }
    };

    fonts.sort();
    fonts.dedup();
    fonts
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn load_fonts() -> Vec<String> {
    tracing::warn!("font listing not supported on this platform");
    Default::default()
}

pub fn fonts() -> Vec<&'static str> {
    FONT_CACHE.iter().map(|s| s.as_str()).collect()
}
