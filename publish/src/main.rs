use {
    clap::Parser,
    dotenv::dotenv,
    lapin::{
        options::{BasicPublishOptions, ConfirmSelectOptions},
        types::{AMQPValue, FieldTable},
        BasicProperties, Connection, ConnectionProperties, Result,
    },
    requestty::{prompt_one, Answer, Question},
    std::iter::Iterator,
    tokio::runtime::Builder,
    tracing::{debug, info, warn},
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
    /// Optional message headers to send (format "key1=value1,key2=value2...")
    #[arg(
        name = "headers",
        long,
        short = 'p',
        value_delimiter = ',',
        env = "PUB_HEADERS"
    )]
    headers: Option<Vec<String>>,
}

impl Cli {
    pub fn url(&self) -> String {
        self.url.to_owned()
    }

    pub fn exchange(&self) -> String {
        self.exchange.to_owned()
    }

    pub fn routing_key(&self) -> String {
        self.routing_key.to_owned().unwrap_or_default()
    }

    pub fn headers(&self) -> Option<FieldTable> {
        match self.headers.as_ref() {
            Some(headers) => Some(headers.iter().fold(FieldTable::default(), |mut ft, s| {
                let mut parts = s.split('=');
                match (parts.next(), parts.next()) {
                    (Some(key), Some(value)) => {
                        debug!("Adding header {} = {}", key, value);
                        ft.insert(key.into(), AMQPValue::LongString(value.into()))
                    }
                    _ => warn!("Ignoring unparsable header value '{}'!", s),
                };
                ft
            })),
            None => None,
        }
    }
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

    let addr = cli.url();
    let exchange = cli.exchange();
    let routing_key = cli.routing_key();
    let properties = match cli.headers() {
        Some(headers) => BasicProperties::default().with_headers(headers),
        None => BasicProperties::default(),
    };

    info!("Connecting to {} {} ...", addr, exchange);

    // create connector to rabbitmq server
    let options = ConnectionProperties::default();
    let connection = runtime.block_on(async {
        let connection = Connection::connect(&addr, options)
            .await
            .expect("Create connection failure!");
        debug!(target="connection", state=?connection.status().state());
        connection
    });

    // create channel with rabbitmq connection
    let channel = runtime.block_on(async {
        let channel = connection
            .create_channel()
            .await
            .expect("Create channel failure!");
        debug!(target="channel", state=?channel.status().state());

        // set channel to publisher-confirms
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await
            .expect("Confirm select failure!");
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
    .expect("Question::confirm failed to return a bool!")
    {
        counter += 1;
        debug!("Sending Message {} ...", counter);

        let confirmed = runtime.block_on(async {
            let payload = format!("Hello person #{:02}!", counter);
            info!("> {}", payload);

            let confirm = channel
                .basic_publish(
                    &exchange,
                    &routing_key,
                    BasicPublishOptions {
                        mandatory: true,
                        ..BasicPublishOptions::default()
                    },
                    payload.as_bytes(),
                    // BasicProperties::default(),
                    properties.to_owned(),
                )
                .await
                .expect("Basic Publish failure!")
                .await
                .expect("Published Confirm failure!");

            if confirm.is_ack() {
                if let Some(message) = confirm.take_message() {
                    warn!(
                        "Messaage rejected with {} {}",
                        message.reply_code, message.reply_text
                    );
                } else {
                    debug!("Message accepted");
                    return true;
                }
            } else if confirm.is_nack() {
                warn!("Message not acknowled!")
            } else {
                warn!("Unknown message state!")
            }
            false
        });
        if confirmed {
            debug!("Message sent");
        } else {
            warn!("message send failed!")
        }
    }
    info!("Finishing off and cleaning up");

    Ok(())
}
