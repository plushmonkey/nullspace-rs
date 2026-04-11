//#![windows_subsystem = "windows"]

fn main() {
    nullspace::run();

    /*
        let (tx, rx) = channel();

        let _ = ctrlc::set_handler(move || {
            let _ = tx.send(());
        });

        let username = "puppet";
        let password = "none";
        let zone = "local";
        let remote_ip = "127.0.0.1";
        let remote_port = 5000;

        let registration = RegistrationFormMessage::new(
            "puppet",
            "puppet@puppet.com",
            "puppet city",
            "puppet state",
            RegistrationSex::Female,
            20,
        );

        let mut client = Client::new(
            username,
            password,
            zone,
            remote_ip,
            remote_port,
            registration,
        )?;

        client.run(rx)?;
    */
}
