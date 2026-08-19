use anyhow::Result;
use clap::Parser;
use owo_colors::OwoColorize;
use std::process::exit;

use license_trace::cli::{AuditArgs, Cli, Commands, ExportArgs, OutputFormat, TraceArgs, WhyArgs};
use license_trace::policy::{CompatibilityReport, CompatibilityStatus};
use license_trace::reporter::{
    AuditReporter, JsonReporter, NoticeReporter, TableReporter, TreeReporter,
};
use license_trace::resolver::{self, NpmOnlineResolver};

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();

    match args.command {
        Commands::Audit(audit_args) => handle_audit(audit_args).await?,
        Commands::Trace(trace_args) => handle_trace(trace_args).await?,
        Commands::Why(why_args) => handle_why(why_args).await?,
        Commands::Export(export_args) => handle_export(export_args).await?,
    }

    Ok(())
}

async fn handle_audit(args: AuditArgs) -> Result<()> {
    let project_dir = std::fs::canonicalize(&args.path)
        .map_err(|_| anyhow::anyhow!("Directory '{}' does not exist", args.path.display()))?;

    let graph = resolver::resolve_auto(&project_dir)?;
    let report = CompatibilityReport::evaluate(&args.outbound, &graph, args.prod_only);

    match args.format {
        OutputFormat::Audit => {
            AuditReporter::render_terminal(&report, &graph);
        }
        OutputFormat::Table => {
            TableReporter::render(&graph, args.prod_only);
        }
        OutputFormat::Tree => {
            TreeReporter::render(&graph);
        }
        OutputFormat::Json => {
            let json_str = JsonReporter::render(&report, &graph)?;
            println!("{}", json_str);
        }
        OutputFormat::Markdown => {
            let md = NoticeReporter::generate_markdown(&graph, args.prod_only);
            println!("{}", md);
        }
    }

    let mut exit_code = 0;
    if args.fail_on_incompatible && report.status == CompatibilityStatus::Incompatible {
        eprintln!(
            "{}",
            "[ERROR] Incompatible license constraints detected in dependencies."
                .bold()
                .red()
        );
        exit_code = 1;
    }

    if args.fail_on_unknown
        && (report.status == CompatibilityStatus::NeedsReview
            || report.obligations.unknown_license_count > 0)
    {
        eprintln!(
            "{}",
            "[WARN] Unknown licenses require manual review before passing audit."
                .bold()
                .yellow()
        );
        if exit_code == 0 {
            exit_code = 2;
        }
    }

    if exit_code != 0 {
        exit(exit_code);
    }

    Ok(())
}

async fn handle_trace(args: TraceArgs) -> Result<()> {
    let target = args.target.trim();
    let path_obj = std::path::Path::new(target);

    let graph = if target == "." || path_obj.exists() {
        // 1. ローカルディレクトリの解析
        let project_dir = std::fs::canonicalize(path_obj)
            .map_err(|_| anyhow::anyhow!("Directory '{}' does not exist", path_obj.display()))?;
        println!("Tracing local project at '{}'...", project_dir.display());
        resolver::resolve_auto(&project_dir)?
    } else if target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("git@")
    {
        // 2. Git リポジトリ URL の解析 (shallow clone して監査)
        if target.starts_with('-') {
            anyhow::bail!("Invalid git repository URL '{}'", target);
        }
        println!(
            "Cloning remote git repository '{}'...",
            target.bold().cyan()
        );
        let temp_dir = std::env::temp_dir().join(format!(
            "license-trace-git-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let status = std::process::Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "--",
                target,
                temp_dir.to_str().unwrap(),
            ])
            .output()?;

        if !status.status.success() {
            anyhow::bail!(
                "Failed to clone git repository: {}",
                String::from_utf8_lossy(&status.stderr)
            );
        }

        let res = resolver::resolve_auto(&temp_dir);
        let _ = std::fs::remove_dir_all(&temp_dir); // クリーンアップ
        res?
    } else {
        // 3. オンライン公開レジストリからの解決 (npm / PyPI)
        let (pkg_name, version) = parse_pkg_spec(target);
        println!(
            "Resolving dependency tree for {} via registry API...",
            target.bold().cyan()
        );

        let resolver = NpmOnlineResolver::new(args.max_depth);
        match resolver.resolve_package(pkg_name, version.as_deref()).await {
            Ok(g) => g,
            Err(e) => anyhow::bail!("Failed to resolve package: {}", e),
        }
    };

    let report = CompatibilityReport::evaluate(&args.outbound, &graph, args.prod_only);

    match args.format {
        OutputFormat::Audit => {
            AuditReporter::render_terminal(&report, &graph);
        }
        OutputFormat::Table => {
            TableReporter::render(&graph, args.prod_only);
        }
        OutputFormat::Tree => {
            TreeReporter::render(&graph);
        }
        OutputFormat::Json => {
            let json_str = JsonReporter::render(&report, &graph)?;
            println!("{}", json_str);
        }
        OutputFormat::Markdown => {
            let md = NoticeReporter::generate_markdown(&graph, args.prod_only);
            println!("{}", md);
        }
    }

    let mut exit_code = 0;
    if args.fail_on_incompatible && report.status == CompatibilityStatus::Incompatible {
        eprintln!(
            "{}",
            "[ERROR] Incompatible license constraints detected in dependencies."
                .bold()
                .red()
        );
        exit_code = 1;
    }

    if args.fail_on_unknown
        && (report.status == CompatibilityStatus::NeedsReview
            || report.obligations.unknown_license_count > 0)
    {
        eprintln!(
            "{}",
            "[WARN] Unknown licenses require manual review before passing audit."
                .bold()
                .yellow()
        );
        if exit_code == 0 {
            exit_code = 2;
        }
    }

    if exit_code != 0 {
        exit(exit_code);
    }

    Ok(())
}

async fn handle_export(args: ExportArgs) -> Result<()> {
    let project_dir = std::fs::canonicalize(&args.path)
        .map_err(|_| anyhow::anyhow!("Directory '{}' does not exist", args.path.display()))?;

    println!(
        "Generating third-party notices for project at '{}'...",
        project_dir.display()
    );
    let graph = resolver::resolve_auto(&project_dir)?;
    let content = NoticeReporter::generate_markdown(&graph, args.prod_only);

    if let Some(out_path) = args.output {
        std::fs::write(&out_path, &content)?;
        println!(
            "{} Successfully exported third-party notices to '{}'",
            "[SUCCESS]".green().bold(),
            out_path.display().to_string().cyan()
        );
    } else {
        println!("{}", content);
    }

    Ok(())
}

async fn handle_why(args: WhyArgs) -> Result<()> {
    let project_dir = std::fs::canonicalize(&args.path)
        .map_err(|_| anyhow::anyhow!("Directory '{}' does not exist", args.path.display()))?;

    let graph = resolver::resolve_auto(&project_dir)?;
    let paths = graph.find_all_paths_to(&args.package);

    println!();
    println!(
        "Dependency path search for '{}':",
        args.package.bold().cyan()
    );
    println!();

    if paths.is_empty() {
        println!(
            "    No dependency paths found reaching '{}'. (The package is not present in graph)",
            args.package.yellow()
        );
    } else {
        println!(
            "    Found {} path(s) from root [{}]:",
            paths.len().to_string().bold().green(),
            graph.root_id.to_string_repr().bold()
        );
        println!();

        for (i, path) in paths.iter().enumerate() {
            let path_str = path
                .iter()
                .enumerate()
                .map(|(idx, id)| {
                    if idx == 0 {
                        id.name.bold().cyan().to_string()
                    } else if idx == path.len() - 1 {
                        id.to_string_repr().bold().yellow().to_string()
                    } else {
                        id.to_string_repr().white().to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ➔ ");

            println!("    Route {:02}: {}", i + 1, path_str);
        }
    }

    println!();
    Ok(())
}

fn parse_pkg_spec(pkg: &str) -> (&str, Option<String>) {
    let trimmed = pkg.trim();
    if let Some(stripped) = trimmed.strip_prefix('@') {
        // スコープ付きパッケージ (例: @angular/core@17.0.0)
        if let Some(second_at_idx) = stripped.find('@') {
            let split_idx = 1 + second_at_idx;
            (
                &trimmed[..split_idx],
                Some(trimmed[split_idx + 1..].to_string()),
            )
        } else {
            (trimmed, None)
        }
    } else if let Some((name, ver)) = trimmed.split_once('@') {
        (name, Some(ver.to_string()))
    } else {
        (trimmed, None)
    }
}
