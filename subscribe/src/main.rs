use {
    clap::Parser,
    dotenv::dotenv,
    joinery::JoinableIterator,
    lapin::{
        message::DeliveryResult,
        options::{BasicAckOptions, BasicConsumeOptions},
        types::{FieldTable, LongString},
        Connection, ConnectionProperties, Result,
    },
    std::str,
    tokio::signal,
    tracing::{debug, error, info, warn},
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
        .expect("Create connection failure!");
    debug!(target="connection", state=?connection.status().state());

    let channel = connection
        .create_channel()
        .await
        .expect("Create channel failure!");
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
                warn!("Failed to consume queue message {}", error);
                return;
            }
        };

        info!(
            "\nReceived {} :: {:?}\n{}",
            &delivery.routing_key,
            match &delivery.properties.headers().as_ref() {
                Some(headers) => headers
                    .into_iter()
                    .map(|(k, v)| {
                        format!(
                            "{}={}",
                            k,
                            v.as_long_string().unwrap_or(&LongString::from(""))
                        )
                    })
                    .join_with(", ")
                    .to_string(),
                None => String::from(""),
            },
            str::from_utf8(&delivery.data[..]).unwrap()
        );

        delivery
            .ack(BasicAckOptions::default())
            .await
            .expect("Message acknowledgement failed!");
    });
    info!("Listening for messages");

    match signal::ctrl_c().await {
        Ok(()) => {
            channel.close(0, "OK").await?;
            connection.close(0, "OK").await?;
        }
        Err(err) => {
            error!("Unable to listen for shutdown signal: {}", err);
        }
    }
    info!("Shutting Down");

    Ok(())
}
