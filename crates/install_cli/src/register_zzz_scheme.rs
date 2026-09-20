use client::ZZZ_URL_SCHEME;
use gpui::{AsyncApp, actions};

actions!(
    cli,
    [
        /// Registers the zzz:// URL scheme handler.
        RegisterZZZScheme
    ]
);

pub async fn register_zzz_scheme(cx: &AsyncApp) -> anyhow::Result<()> {
    cx.update(|cx| cx.register_url_scheme(ZZZ_URL_SCHEME)).await
}
