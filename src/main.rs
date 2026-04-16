//#![windows_subsystem = "windows"]

use nullspace::{ApplicationConfig, ApplicationEvent, EventProcessor};

use winit::event_loop::EventLoop;

use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long, default_value = "nullspace")]
    username: String,
    #[arg(short, long, default_value = "password")]
    password: String,
    #[arg(long, default_value = "127.0.0.1")]
    ip: String,
    #[arg(long, default_value = "5000")]
    port: u16,
}

fn main() {
    let args = Args::parse();

    let config = ApplicationConfig::new_exe(args.ip, args.port, args.username, args.password);

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
