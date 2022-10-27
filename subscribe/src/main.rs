use {
    clap::Parser,
    dotenv::dotenv,
    lapin::{
        message::DeliveryResult,
        options::{BasicAckOptions, BasicConsumeOptions},
        types::FieldTable,
        Connection, ConnectionProperties, Result,
    },
    std::str,
    tokio::signal,
    tracing::{debug, info},
};

#[derive(Parser, Debug)]
struct Cli {
    /// URL of the RabbitMQ server
    #[arg(
        name = "url",
        long,
        short,
        env = "AMQP_ADDR",
        default_value_t = String::from("amqp://localhost:5672")
    )]
    url: String,
    /// Queue to subscribe to
    #[arg(name = "queue", long, short, env = "SUB_QUEUE")]
    queue: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "debug");
    }
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    info!("Starting up");
    let addr = &cli.url;
    let queue = &cli.queue;
    info!("Connecting to {} {} ...", addr, queue);

    let options = ConnectionProperties::default()
        // Use tokio executor and reactor.
        // At the moment the reactor is only available for unix.
        .with_executor(tokio_executor_trait::Tokio::current())
        .with_reactor(tokio_reactor_trait::Tokio);

    let connection = Connection::connect(addr, options)
        .await
        .expect("connection error");
    debug!(target="connection", state=?connection.status().state());

    let channel = connection
        .create_channel()
        .await
        .expect("create_channel: send");
    debug!(target="channel", state=?channel.status().state());

    info!("Connected to server!");

    let consumer = channel
        .basic_consume(
            &queue,
            "",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;
    info!("Subscribed to queue {}!", cli.queue);

    consumer.set_delegate(move |delivery: DeliveryResult| async move {
        let delivery = match delivery {
            // Carries the delivery alongside its channel
            Ok(Some(delivery)) => delivery,
            // The consumer got canceled
            Ok(None) => {
                return;
            }
            // Carries the error and is always followed by Ok(None)
            Err(error) => {
                debug!("Failed to consume queue message {}", error);
                return;
            }
        };

        let data = &delivery.data[..];
        info!(
            "Received {} :: {}",
            delivery.routing_key,
            str::from_utf8(data).unwrap()
        );

        delivery
            .ack(BasicAckOptions::default())
            .await
            .expect("Failed to ack send_webhook_event message");
    });
    info!("Listening for messages");

    match signal::ctrl_c().await {
        Ok(()) => {
            channel.close(0, "OK").await?;
            connection.close(0, "OK").await?;
        }
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }
    info!("Shutting Down");

    Ok(())
}
