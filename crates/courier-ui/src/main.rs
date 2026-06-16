mod app;
mod components;
mod theme;
mod views;

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "courier_ui=info,courier_app=info".into()),
        )
        .init();

    iced::daemon(app::title, app::update, app::view)
        .theme(app::theme)
        .subscription(app::subscription)
        .run_with(app::init)
}
