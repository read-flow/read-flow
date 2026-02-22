use std::process::Command;
use std::sync::LazyLock;

static FONT_CACHE: LazyLock<Vec<String>> = LazyLock::new(load_fonts);

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

pub fn fonts() -> Vec<&'static str> {
    FONT_CACHE.iter().map(|s| s.as_str()).collect()
}
