use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Deploy a Synchronic Web Journal Software Development Kit (SDK) as a local webserver"
)]
pub struct Config {
    #[arg(short, long, default_value_t = String::from(""), help = "Path to the persistent database")]
    pub database: String,

    #[arg(
        short,
        long,
        default_value_t = 4096,
        help = "Port to access the webserver"
    )]
    pub port: u16,

    #[arg(short, long, default_value_t = String::from(""), help = "Initial script to pass into the Journal")]
    pub boot: String,

    #[arg(short, long, default_value_t = String::from(""), help = "If set, then evaluate the provided query, print, and exit immediately")]
    pub evaluate: String,

    #[arg(short, long, default_value_t = String::from(""), help = "Contents of the step query")]
    pub step: String,

    #[arg(
        short = 'c',
        long,
        default_value_t = 1.0,
        help = "Number of seconds between each step inquiry"
    )]
    pub period: f64,

    #[arg(long, default_value_t = String::from(""), hide = true)]
    pub secret: String,

    #[arg(long, default_value_t = false, hide = true)]
    pub update_records: bool,

    #[arg(long, default_value_t = String::from(""), hide = true)]
    pub window: String,

    #[arg(long, default_value_t = String::from(""), hide = true)]
    pub interface: String,

    #[arg(long, default_value_t = String::from(""), hide = true)]
    pub name: String,

    #[arg(long, default_value_t = String::from("push"), hide = true)]
    pub bridge_publish: String,

    #[arg(long, default_value_t = String::from("pull"), hide = true)]
    pub bridge_subscribe: String,
}

impl Config {
    pub fn new() -> Self {
        let mut config = Config::parse();
        if config.database.is_empty() {
            if let Ok(database) = std::env::var("SYNC_WEB_DATABASE") {
                config.database = database;
            }
        }
        config
    }
}
