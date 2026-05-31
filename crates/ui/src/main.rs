mod app;
mod components;
mod theme;
mod views;

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mailspring_ui=info,mailcore=info".into()),
        )
        .init();

    iced::application("MailSpring Rust", app::update, app::view)
        .theme(app::theme)
        .run_with(app::init)
}
