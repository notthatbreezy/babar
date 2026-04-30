//! Small CLI example backed by babar.
//!
//! ```text
//! cargo run -p babar --example todo_cli -- --help
//! ```

use std::process::ExitCode;

use babar::codec::{bool, int4, text};
use babar::query::{Command, Query};
use babar::{Config, Session};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
struct Cli {
    #[arg(long, env = "PGHOST", default_value = "127.0.0.1")]
    host: String,
    #[arg(long, env = "PGPORT", default_value_t = 5432)]
    port: u16,
    #[arg(long, env = "PGUSER", default_value = "postgres")]
    user: String,
    #[arg(long, env = "PGDATABASE", default_value = "postgres")]
    database: String,
    #[arg(long, env = "PGPASSWORD", default_value = "postgres")]
    password: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init,
    Add { id: i32, title: String },
    Done { id: i32 },
    List,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let cfg = Config::new(&cli.host, cli.port, &cli.user, &cli.database)
        .password(cli.password)
        .application_name("babar-todo-cli");

    match run(cfg, cli.command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

async fn run(config: Config, command: Commands) -> babar::Result<()> {
    let session = Session::connect(config).await?;
    match command {
        Commands::Init => init(&session).await?,
        Commands::Add { id, title } => add(&session, id, title).await?,
        Commands::Done { id } => mark_done(&session, id).await?,
        Commands::List => list(&session).await?,
    }
    session.close().await
}

async fn init(session: &Session) -> babar::Result<()> {
    let create: Command<()> = Command::raw(
        "CREATE TABLE IF NOT EXISTS todo_items (id int4 PRIMARY KEY, title text NOT NULL, done bool NOT NULL DEFAULT false)");
    session.execute(&create, ()).await?;
    println!("todo_items is ready");
    Ok(())
}

async fn add(session: &Session, id: i32, title: String) -> babar::Result<()> {
    let insert: Command<(i32, String)> = Command::raw_with(
        "INSERT INTO todo_items (id, title) VALUES ($1, $2)",
        (int4, text),
    );
    session.execute(&insert, (id, title.clone())).await?;
    println!("added #{id}: {title}");
    Ok(())
}

async fn mark_done(session: &Session, id: i32) -> babar::Result<()> {
    let update: Command<(i32,)> =
        Command::raw_with("UPDATE todo_items SET done = true WHERE id = $1", (int4,));
    let updated = session.execute(&update, (id,)).await?;
    println!("marked {updated} row(s) done");
    Ok(())
}

async fn list(session: &Session) -> babar::Result<()> {
    let select: Query<(), (i32, String, bool)> = Query::raw(
        "SELECT id, title, done FROM todo_items ORDER BY id",
        (int4, text, bool),
    );
    for (id, title, done) in session.query(&select, ()).await? {
        let marker = if done { 'x' } else { ' ' };
        println!("[{marker}] {id:>4} {title}");
    }
    Ok(())
}
