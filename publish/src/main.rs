use {
    clap::Parser,
    dotenv::dotenv,
    lapin::{
        options::BasicPublishOptions, BasicProperties, Connection, ConnectionProperties, Result,
    },
    requestty::{prompt_one, Answer, Question},
    tokio::runtime::Builder,
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
    /// Exchange to publish to
    #[arg(name = "exchange", long, short = 'x', env = "PUB_EXCHANGE")]
    exchange: String,
    /// The routing key to use
    #[arg(name = "routing-key", long, short, env = "PUB_ROUTING")]
    routing_key: Option<String>,
}

fn main() -> Result<()> {
    dotenv().ok();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    tracing_subscriber::fmt::init();

    // create async runtime (tokio)
    let runtime = Builder::new_current_thread().enable_all().build()?;

    let cli = Cli::parse();

    info!("Starting up");

    let addr = cli.url;
    let exchange = cli.exchange;
    let routing_key = cli.routing_key.unwrap_or_default();
    info!("Connecting to {} {} ...", addr, exchange);

    // create connector to rabbitmq server
    let options = ConnectionProperties::default();
    let connection = runtime.block_on(async {
        let connection = Connection::connect(&addr, options)
            .await
            .expect("connection error");
        debug!(target="connection", state=?connection.status().state());
        connection
    });

    // create channel with rabbitmq connection
    let channel = runtime.block_on(async {
        let channel = connection
            .create_channel()
            .await
            .expect("create_channel: send");
        debug!(target="channel", state=?channel.status().state());
        channel
    });

    let mut counter: i32 = 0;

    while prompt_one(
        Question::confirm("send")
            .message("Do you with to send a message?")
            .default(true)
            .build(),
    )
    .unwrap_or(Answer::Bool(false))
    .as_bool()
    .expect("Question::confirm returns a bool")
    {
        counter += 1;
        info!("Sending Message {} ...", counter);
        let _confirm = runtime.block_on(async {
            let payload = format!("Hello person #{:02}!", counter);
            info!("> {}", payload);

            channel
                .basic_publish(
                    &exchange,
                    &routing_key,
                    BasicPublishOptions::default(),
                    payload.as_bytes(),
                    BasicProperties::default(),
                )
                .await?
                .await
        });
        info!("message sent!");
    }
    info!("Finishing off and cleaning up");

    Ok(())
}
