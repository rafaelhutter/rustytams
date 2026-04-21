use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "rustytams", about = "Time-addressable Media Store server")]
pub struct Config {
    /// Port to listen on
    #[arg(long, default_value_t = 5800)]
    pub port: u16,

    /// MongoDB connection URI
    #[arg(long, env = "MONGO_URI", default_value = "mongodb://localhost:27017")]
    pub mongo_uri: String,

    /// S3 endpoint URL (e.g. http://localhost:9000 for RustFS)
    #[arg(long, env = "B2_ENDPOINT", default_value = "http://localhost:9000")]
    pub s3_endpoint: String,

    /// S3 bucket name for media objects
    #[arg(long, env = "B2_BUCKET", default_value = "tams-media")]
    pub s3_bucket: String,

    /// S3 access key
    #[arg(long, env = "B2_ACCESS_KEY_ID", default_value = "rustfsadmin")]
    pub s3_access_key: String,

    /// S3 secret key
    #[arg(long, env = "B2_SECRET_ACCESS_KEY", default_value = "rustfsadmin123")]
    pub s3_secret_key: String,

    /// S3 region
    #[arg(long, env = "B2_REGION", default_value = "us-east-1")]
    pub s3_region: String,

    /// Base URL of the auth server (e.g. http://localhost:5802)
    #[arg(long, env = "AUTH_URL", default_value = "http://localhost:5802")]
    pub auth_url: String,
}
