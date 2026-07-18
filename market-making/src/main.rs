//! Mr. Market — Simulated Bookmaking Operation with MLE Position Inference
//!
//! CLI entry point that orchestrates the full pipeline:
//!   download → parse → store features → fit GMM → run simulation → infer MLE → print report
//!
//! Based on the Korean Order Book Specification (korean-order-book-spec.md).

mod bookmaker;
mod gmm;
mod iex;
mod memorydb;
mod mle;
mod ofi;
mod portfolio;
mod pricer;
mod risk_gate;
mod simulation;
mod sofr;
mod symbology;
mod vol_surface;

use clap::{Parser, Subcommand};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::gmm::em::EmConfig;
use crate::iex::downloader::{IexDownloader, IexFeed};
use crate::memorydb::vector_store::VectorStore;
use crate::memorydb::MemoryDbConfig;
use crate::simulation::{MrMarketSimulation, SimulationConfig};

/// Mr. Market — Simulated bookmaking operation with GMM hidden-state MLE position inference.
#[derive(Parser, Debug)]
#[command(name = "mr-market")]
#[command(version = "0.1.0")]
#[command(about = "Simulated Mr. Market bookmaking operation", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the full simulation pipeline
    Run {
        /// Symbol to simulate (e.g. AAPL)
        #[arg(short, long, default_value = "AAPL")]
        symbol: String,

        /// Average daily volume (shares) for feature normalization
        #[arg(short = 'v', long, default_value_t = 10_000_000.0)]
        adv: f64,

        /// SOFR base rate (e.g. 0.0535 for 5.35%)
        #[arg(short, long, default_value_t = 0.0535)]
        sofr: f64,

        /// Risk aversion parameter γ
        #[arg(short = 'g', long, default_value_t = 0.015)]
        gamma: f64,

        /// Margin haircut
        #[arg(long, default_value_t = 0.15)]
        margin_haircut: f64,

        /// Borrow premium
        #[arg(long, default_value_t = 0.0025)]
        borrow_premium: f64,

        /// Liquidity parameter κ
        #[arg(short = 'k', long, default_value_t = 2.1)]
        kappa: f64,

        /// Time to horizon (fraction of day)
        #[arg(short = 't', long, default_value_t = 0.45)]
        time_to_horizon: f64,

        /// Max order quantity (risk gate)
        #[arg(long, default_value_t = 1000)]
        max_qty: u32,

        /// Max price USD (risk gate)
        #[arg(long, default_value_t = 5000.0)]
        max_price: f64,

        /// Max absolute delta (risk gate)
        #[arg(long, default_value_t = 5000.0)]
        max_delta: f64,

        /// MLE grid search lower bound
        #[arg(long, default_value_t = -5000.0)]
        q_min: f64,

        /// MLE grid search upper bound
        #[arg(long, default_value_t = 5000.0)]
        q_max: f64,

        /// MLE grid resolution
        #[arg(long, default_value_t = 200)]
        n_grid: usize,

        /// Fill probability when our quote is crossed (0..1)
        #[arg(long, default_value_t = 0.3)]
        fill_prob: f64,

        /// Path to IEX PCAP or CSV file (if omitted, uses synthetic data)
        #[arg(short, long)]
        data_file: Option<String>,

        /// Number of synthetic events to generate (if no data file)
        #[arg(short, long, default_value_t = 5000)]
        n_events: usize,

        /// MemoryDB endpoint (if omitted, uses in-memory store)
        #[arg(long)]
        memorydb_endpoint: Option<String>,

        /// MemoryDB port
        #[arg(long, default_value_t = 6379)]
        memorydb_port: u16,

        /// Use TLS for MemoryDB connection
        #[arg(long, default_value_t = false)]
        memorydb_tls: bool,

        /// MemoryDB auth token
        #[arg(long)]
        memorydb_token: Option<String>,

        /// Output JSON report to file (if omitted, prints to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Download IEX historical data
    Download {
        /// Date to download (YYYYMMDD format)
        #[arg(short, long)]
        date: String,

        /// Feed type: tops or deep
        #[arg(short, long, default_value = "tops")]
        feed: String,

        /// Output directory
        #[arg(short, long, default_value = "./data/iex")]
        output_dir: String,
    },

    /// Run a quick synthetic test
    Test {
        /// Number of synthetic events
        #[arg(short, long, default_value_t = 1000)]
        n_events: usize,
    },
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            symbol,
            adv,
            sofr,
            gamma,
            margin_haircut,
            borrow_premium,
            kappa,
            time_to_horizon,
            max_qty,
            max_price,
            max_delta,
            q_min,
            q_max,
            n_grid,
            fill_prob,
            data_file,
            n_events,
            memorydb_endpoint,
            memorydb_port,
            memorydb_tls,
            memorydb_token,
            output,
        } => {
            run_simulation(SimulationArgs {
                symbol,
                adv,
                sofr,
                gamma,
                margin_haircut,
                borrow_premium,
                kappa,
                time_to_horizon,
                max_qty,
                max_price,
                max_delta,
                q_min,
                q_max,
                n_grid,
                fill_prob,
                data_file,
                n_events,
                memorydb_endpoint,
                memorydb_port,
                memorydb_tls,
                memorydb_token,
                output,
            })
            .await
        }

        Commands::Download {
            date,
            feed,
            output_dir,
        } => {
            let feed_type = match feed.to_lowercase().as_str() {
                "tops" => IexFeed::Tops,
                "deep" => IexFeed::Deep,
                _ => {
                    error!("Unknown feed type: {}. Use 'tops' or 'deep'", feed);
                    std::process::exit(1);
                }
            };

            let downloader = IexDownloader::new(&output_dir);
            match downloader.download(&date, feed_type) {
                Ok(path) => {
                    info!("Downloaded to: {}", path.display());
                    println!("✓ Downloaded {} {} data to {}", date, feed, path.display());
                }
                Err(e) => {
                    error!("Download failed: {}", e);
                    println!("✗ Download failed: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Test { n_events } => {
            info!("Running quick synthetic test with {} events", n_events);
            let config = SimulationConfig {
                symbol: "TEST".to_string(),
                adv: 1_000_000.0,
                fill_probability: 0.5,
                ..Default::default()
            };
            let mut sim = MrMarketSimulation::new(config);
            let mut store = VectorStore::in_memory();

            let result = sim.run_synthetic(n_events, &mut store).await;
            print_report(&result);
        }
    }
}

/// Arguments for the `run` subcommand.
struct SimulationArgs {
    symbol: String,
    adv: f64,
    sofr: f64,
    gamma: f64,
    margin_haircut: f64,
    borrow_premium: f64,
    kappa: f64,
    time_to_horizon: f64,
    max_qty: u32,
    max_price: f64,
    max_delta: f64,
    q_min: f64,
    q_max: f64,
    n_grid: usize,
    fill_prob: f64,
    data_file: Option<String>,
    n_events: usize,
    memorydb_endpoint: Option<String>,
    memorydb_port: u16,
    memorydb_tls: bool,
    memorydb_token: Option<String>,
    output: Option<String>,
}

async fn run_simulation(args: SimulationArgs) {
    info!("Configuring Mr. Market simulation for {}", args.symbol);

    let config = SimulationConfig {
        symbol: args.symbol.clone(),
        adv: args.adv,
        sofr_rate: args.sofr,
        risk_aversion: args.gamma,
        margin_haircut: args.margin_haircut,
        borrow_premium: args.borrow_premium,
        liquidity_kappa: args.kappa,
        time_to_horizon: args.time_to_horizon,
        max_order_qty: args.max_qty,
        max_price_usd: args.max_price,
        max_delta: args.max_delta,
        q_min: args.q_min,
        q_max: args.q_max,
        n_grid: args.n_grid,
        em_config: EmConfig::default(),
        initial_cash: 100_000_000.0,
        fill_probability: args.fill_prob,
        ofi_decay: 0.95,
        ofi_multiplier: 0.001,
    };

    let mut sim = MrMarketSimulation::new(config);

    // Set up vector store
    let mut vector_store = if let Some(endpoint) = &args.memorydb_endpoint {
        info!("Using MemoryDB at {}:{}", endpoint, args.memorydb_port);
        let mdb_config = MemoryDbConfig {
            endpoint: endpoint.clone(),
            port: args.memorydb_port,
            use_tls: args.memorydb_tls,
            auth_token: args.memorydb_token.clone(),
            key_prefix: "mrmarket".to_string(),
        };
        let mut store = VectorStore::memorydb(mdb_config);
        match store.connect().await {
            Ok(_) => info!("Connected to MemoryDB"),
            Err(e) => {
                warn!("Failed to connect to MemoryDB: {}, falling back to in-memory", e);
                VectorStore::in_memory()
            }
        }
    } else {
        info!("Using in-memory vector store (no MemoryDB endpoint specified)");
        VectorStore::in_memory()
    };

    // Run simulation
    let result = if let Some(ref data_file) = args.data_file {
        info!("Loading market data from: {}", data_file);
        match sim.run_from_file(data_file, &mut vector_store).await {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to load data file: {}", e);
                warn!("Falling back to synthetic data ({} events)", args.n_events);
                sim.run_synthetic(args.n_events, &mut vector_store).await
            }
        }
    } else {
        info!("No data file specified, using {} synthetic events", args.n_events);
        sim.run_synthetic(args.n_events, &mut vector_store).await
    };

    // Output results
    if let Some(ref output_path) = args.output {
        match serde_json::to_string_pretty(&result) {
            Ok(json) => {
                if let Err(e) = std::fs::write(output_path, &json) {
                    error!("Failed to write output file: {}", e);
                } else {
                    info!("Report written to {}", output_path);
                }
            }
            Err(e) => error!("Failed to serialize result: {}", e),
        }
    }

    print_report(&result);
}

/// Print a human-readable summary report to stdout.
fn print_report(result: &simulation::SimulationResult) {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║          Mr. Market Simulation Report                            ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
    println!("Symbol:           {}", result.symbol);
    println!("Events processed: {}", result.n_events);
    println!("Features extracted: {}", result.n_features);
    println!();

    // GMM parameters
    println!("── GMM Hidden-State Parameters ──────────────────────────────────");
    println!("  π_noise:          {:.6}  (noise trader)", result.gmm.pi_noise());
    println!("  π_institutional:  {:.6}  (institutional)", result.gmm.pi_institutional());
    println!("  π_informed:       {:.6}  (informed insider)", result.gmm.pi_informed());
    println!("  Log-likelihood:   {:.4}", result.gmm.log_likelihood);
    println!("  Converged:        {}", result.gmm.converged);
    println!("  EM iterations:    {}", result.gmm.n_iterations);
    println!();

    // MLE position inference
    println!("── MLE Position Inference ───────────────────────────────────────");
    println!("  Optimal q*:           {:.2} shares", result.mle_result.optimal_q);
    println!("  Max log-likelihood:  {:.4}", result.mle_result.max_log_likelihood);
    println!("  Expected spread revenue:    ${:>12.2}", result.mle_result.expected_spread_revenue);
    println!("  Expected adverse selection: ${:>12.2}", result.mle_result.expected_adverse_selection);
    println!("  Expected SOFR carry cost:   ${:>12.2}", result.mle_result.expected_sofr_carry);
    println!("  Expected inventory risk:    ${:>12.2}", result.mle_result.expected_inventory_risk);
    println!();

    // P&L breakdown
    println!("── P&L Breakdown ────────────────────────────────────────────────");
    println!("  Spread revenue:      ${:>14.2}", result.pnl.spread_revenue);
    println!("  Adverse selection:  ${:>14.2}", result.pnl.adverse_selection_cost);
    println!("  SOFR carry cost:     ${:>14.2}", result.pnl.sofr_carry_cost);
    println!("  Hedging cost:        ${:>14.2}", result.pnl.hedging_cost);
    println!("  ──────────────────────────────────────");
    println!("  Realized P&L:        ${:>14.2}", result.pnl.realized_pnl);
    println!("  Unrealized P&L:      ${:>14.2}", result.pnl.unrealized_pnl);
    println!("  Total P&L:           ${:>14.2}", result.pnl.total_pnl);
    println!();

    // Activity stats
    println!("── Activity Statistics ──────────────────────────────────────────");
    println!("  Quotes generated:    {}", result.pnl.n_quotes);
    println!("  Fills executed:      {}", result.pnl.n_fills);
    println!("  Hedges executed:     {}", result.pnl.n_hedges);
    println!("  Risk rejections:     {}", result.pnl.n_rejections);
    println!("  Final delta:         {:.2}", result.pnl.final_delta);
    println!("  Final cash:          ${:>14.2}", result.pnl.final_cash);
    println!();

    // Final quote
    if let Some(ref quote) = result.final_quote {
        println!("── Final Quote ──────────────────────────────────────────────────");
        println!("  Bid:    ${:.4}", quote.bid_price);
        println!("  Ask:    ${:.4}", quote.ask_price);
        println!("  Spread: ${:.4}", quote.spread_width);
        println!("  Reservation: ${:.4}", quote.reservation_price);
        println!("  OFI drift:   {:.6}", quote.ofi_drift);
    }

    println!();
    println!("═══════════════════════════════════════════════════════════════════");
}