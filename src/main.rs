//#![windows_subsystem = "windows"]

use nullspace::{ApplicationConfig, ApplicationEvent, EventProcessor};

use winit::event_loop::EventLoop;

use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    username: Option<String>,
    #[arg(short, long)]
    password: Option<String>,
    #[arg(long)]
    ip: Option<String>,
    #[arg(long)]
    port: Option<u16>,
}

fn main() {
    let args = Args::parse();

    let remote_ip = args.ip.unwrap_or("127.0.0.1".to_owned());
    let port = args.port.unwrap_or(5000);
    let username = args.username.unwrap_or("nullspace".to_owned());
    let password = args.password.unwrap_or("password".to_owned());

    let config = ApplicationConfig::new_exe(remote_ip, port, username, password);

    env_logger::Builder::new()
        .filter(None, log::LevelFilter::Warn)
        .filter(Some("nullspace"), log::LevelFilter::Debug)
        .init();

    let event_loop: EventLoop<ApplicationEvent> = EventLoop::with_user_event()
        .build()
        .expect("event loop must be supported on this platform");

    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut event_processor = EventProcessor::new(config, &event_loop);

    event_loop
        .run_app(&mut event_processor)
        .expect("event loop should run");
}
