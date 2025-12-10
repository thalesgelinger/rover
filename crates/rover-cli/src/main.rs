use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use rover_android_runner::AndroidRunner;
use rover_ios_runner::IosRunner;
use rover_runtime::Runtime;

#[derive(Parser, Debug)]
#[command(name = "rover", about = "Lua-first mobile runner", version)]
struct Cli {
    #[arg(value_name = "ENTRY", help = "Lua entry file")]
    entry: Option<PathBuf>,

    #[arg(short = 'p', long = "platform", value_enum, default_value = "ios")]
    platform: Option<Platform>,

    #[arg(long, value_name = "UDID", help = "iOS device UDID")]
    device: Option<String>,

    #[arg(long, help = "Verbose logging")]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    Run(RunOpts),
    Build(RunOpts),
}

#[derive(Args, Debug, Clone)]
struct RunOpts {
    #[arg(value_name = "ENTRY", help = "Lua entry file")]
    entry: PathBuf,

    #[arg(short = 'p', long = "platform", value_enum, default_value = "ios")]
    platform: Platform,

    #[arg(long, value_name = "UDID", help = "iOS device UDID (disables sim)")]
    device: Option<String>,

    #[arg(long, default_value_t = true, help = "Target simulator")]
    sim: bool,

    #[arg(long, help = "Verbose logging")]
    verbose: bool,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Ios,
    Android,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Run(opts)) => run(opts),
        Some(Command::Build(opts)) => build(opts),
        None => {
            let entry = cli.entry.ok_or_else(|| anyhow::anyhow!("ENTRY required"))?;
            let platform = cli.platform.unwrap_or(Platform::Ios);
            let opts = RunOpts {
                entry,
                platform,
                device: cli.device.clone(),
                sim: cli.device.is_none(),
                verbose: cli.verbose,
            };
            run(opts)
        }
    }
}

fn run(opts: RunOpts) -> Result<()> {
    let target = target_desc(&opts);
    println!("[rover] run {} on {}", opts.entry.display(), target);
    let mut runtime = Runtime::new().context("init runtime")?;
    runtime
        .load_entry(&opts.entry)
        .with_context(|| format!("load {}", opts.entry.display()))?;
    runtime.init_state().context("init state")?;
    let view = runtime.render_view().context("render app")?;
    println!("[rover] render output: {:?}", view);

    dispatch_platform_run(&opts, &opts.entry)?;
    Ok(())
}

fn build(opts: RunOpts) -> Result<()> {
    let target = target_desc(&opts);
    println!("[rover] build {} for {}", opts.entry.display(), target);
    dispatch_platform_build(&opts, &opts.entry)
}

fn target_desc(opts: &RunOpts) -> String {
    match (opts.platform, opts.device.as_ref(), opts.sim) {
        (Platform::Ios, Some(udid), _) => format!("ios device {udid}"),
        (Platform::Ios, None, true) => "ios simulator".to_string(),
        (Platform::Ios, None, false) => "ios (unspecified target)".to_string(),
        (Platform::Android, Some(_), _) => "android device".to_string(),
        (Platform::Android, None, _) => "android emulator".to_string(),
    }
}

fn dispatch_platform_run(opts: &RunOpts, entry: &PathBuf) -> Result<()> {
    match opts.platform {
        Platform::Ios => {
            let runner = IosRunner::new();
            runner.ensure_prereqs()?;
            runner.build_and_run_sim(entry)?;
            Ok(())
        }
        Platform::Android => {
            let runner = AndroidRunner::new();
            runner.build_and_run(entry)
        }
    }
}

fn dispatch_platform_build(opts: &RunOpts, entry: &PathBuf) -> Result<()> {
    match opts.platform {
        Platform::Ios => {
            let runner = IosRunner::new();
            runner.ensure_prereqs()?;
            runner.stage_payload(entry)?;
            runner.generate_project()?;
            Ok(())
        }
        Platform::Android => {
            let runner = AndroidRunner::new();
            runner.ensure_prereqs()?;
            let _project = runner.generate_project()?;
            runner.stage_payload(entry)?;
            let lib = runner.build_rust_shared()?;
            let _apk = runner.build_apk(&lib)?;
            Ok(())
        }
    }
}
