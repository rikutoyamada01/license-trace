use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "license-trace",
    author,
    version,
    about = "Recursive license tracer, obligations lower-bound evaluator, and OSS compliance analyzer",
    long_about = "Traces recursive dependencies and analyzes license compatibility (e.g. MIT readiness), obligations, and path origins."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Audit a local project for license compliance and aggregate obligations against an outbound license (e.g., MIT)
    Audit(AuditArgs),

    /// Trace a remote package and its recursive transitive dependencies online via registry API
    Trace(TraceArgs),

    /// Explain why a dependency is included in the project by showing all arrival paths
    Why(WhyArgs),

    /// Export aggregated third-party notices and licenses into THIRD_PARTY_LICENSES.md
    Export(ExportArgs),
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Audit,
    Table,
    Tree,
    Json,
    Markdown,
}

#[derive(Args, Debug)]
pub struct AuditArgs {
    /// Path to project root directory
    #[arg(short, long, default_value = ".")]
    pub path: PathBuf,

    /// Outbound license you intend to release under
    #[arg(short, long, default_value = "MIT")]
    pub outbound: String,

    /// Only analyze production dependencies (exclude devDependencies)
    #[arg(long)]
    pub prod_only: bool,

    /// Output format
    #[arg(short, long, value_enum, default_value = "audit")]
    pub format: OutputFormat,

    /// Exit with code 1 if incompatible licenses are detected
    #[arg(long)]
    pub fail_on_incompatible: bool,

    /// Exit with code 1 if unknown or un-evaluated licenses are detected
    #[arg(long)]
    pub fail_on_unknown: bool,
}

#[derive(Args, Debug)]
pub struct TraceArgs {
    /// Target to trace: local directory (e.g. '.', './my-app'), remote package ('express', 'lodash@4.17.21'), or git repository URL
    #[arg(default_value = ".")]
    pub target: String,

    /// Maximum dependency resolution depth
    #[arg(short, long, default_value = "10")]
    pub max_depth: usize,

    /// Outbound target license to evaluate against
    #[arg(short, long, default_value = "MIT")]
    pub outbound: String,

    /// Only analyze production dependencies
    #[arg(long)]
    pub prod_only: bool,

    /// Output format
    #[arg(short, long, value_enum, default_value = "audit")]
    pub format: OutputFormat,

    /// Exit with code 1 if incompatible licenses are detected
    #[arg(long)]
    pub fail_on_incompatible: bool,

    /// Exit with code 2 if unknown licenses are detected
    #[arg(long)]
    pub fail_on_unknown: bool,
}

#[derive(Args, Debug)]
pub struct WhyArgs {
    /// Package name to trace the path for
    pub package: String,

    /// Path to project root directory
    #[arg(short, long, default_value = ".")]
    pub path: PathBuf,
}

#[derive(Args, Debug)]
pub struct ExportArgs {
    /// Target project directory to export licenses for
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Output file path (e.g. THIRD_PARTY_LICENSES.md). If omitted, prints to stdout.
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Only include production dependencies
    #[arg(long)]
    pub prod_only: bool,
}
