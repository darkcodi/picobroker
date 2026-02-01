use picobroker_tokio::MqttServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    const MAX_SESSIONS: usize = 4;
    const MAX_TOPICS: usize = 32;

    type MyMqttServer = MqttServer<64, 256, 8, MAX_SESSIONS, MAX_TOPICS, 8>;
    let server = MyMqttServer::new();

    log::info!("Starting picobroker tokio server on 0.0.0.0:1883");
    log::info!("Configuration: {} max sessions, {} max topics", MAX_SESSIONS, MAX_TOPICS);

    server.run("0.0.0.0:1883").await?;

    Ok(())
}
