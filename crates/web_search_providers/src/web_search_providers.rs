use gpui::App;
use web_search::WebSearchRegistry;

pub fn init(cx: &mut App) {
    let _registry = WebSearchRegistry::global(cx);
}
