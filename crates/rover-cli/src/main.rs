use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use notify_debouncer_full::{new_debouncer, notify::{RecursiveMode, Watcher}, DebounceEventResult};
use rover_android_runner::AndroidRunner;
use rover_devserver::{write_config, DevConfig, DevServer};
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

    #[arg(long, help = "Watch for file changes and hot reload")]
    watch: bool,

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

    #[arg(long, help = "Watch for file changes and hot reload")]
    watch: bool,
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
                watch: cli.watch,
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

    if opts.watch {
        let devserver = DevServer::start().context("start devserver")?;
        let host = match opts.platform {
            Platform::Ios => "127.0.0.1".to_string(),
            Platform::Android => "10.0.2.2".to_string(),
        };
        if let Some(parent) = opts.entry.parent() {
            let cfg = DevConfig { host: host.clone(), port: devserver.port() };
            write_config(parent, &cfg)?;
            println!("[rover] devserver at {}:{}", host, cfg.port);
        }
        start_file_watcher(&opts.entry, devserver)?;
        println!("[rover] watching {} for changes...", opts.entry.display());
        println!("[rover] press ctrl+c to stop");
        // Give devserver time to bind before launching app
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    dispatch_platform_run(&opts, &opts.entry)?;
    
    if opts.watch {
        // Keep CLI alive to show reload logs
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    
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

fn start_file_watcher(entry: &PathBuf, devserver: DevServer) -> Result<()> {
    let watch_dir = entry
        .parent()
        .ok_or_else(|| anyhow::anyhow!("entry has no parent dir"))?
        .to_path_buf();
    
    let (tx, rx) = channel();
    
    let mut debouncer = new_debouncer(
        Duration::from_millis(200),
        None,
        move |result: DebounceEventResult| {
            if let Ok(events) = result {
                for event in events {
                    for path in &event.paths {
                        if path.extension().and_then(|s| s.to_str()) == Some("lua") {
                            tx.send(()).ok();
                            break;
                        }
                    }
                }
            }
        },
    )
    .context("create file watcher")?;

    debouncer
        .watcher()
        .watch(&watch_dir, RecursiveMode::Recursive)
        .with_context(|| format!("watch {}", watch_dir.display()))?;

    let watch_dir_clone = watch_dir.clone();
    std::thread::spawn(move || {
        let _debouncer = debouncer;
        loop {
            if rx.recv().is_ok() {
                println!("[rover] file changed, triggering reload...");
                match collect_lua_files(&watch_dir_clone) {
                    Ok(files) => {
                        if let Err(e) = devserver.trigger(files) {
                            eprintln!("[rover] reload trigger failed: {e}");
                        }
                    }
                    Err(e) => eprintln!("[rover] failed to collect files: {e}"),
                }
            }
        }
    });

    Ok(())
}

fn collect_lua_files(dir: &PathBuf) -> Result<std::collections::HashMap<String, Vec<u8>>> {
    use std::collections::HashMap;
    use std::fs;
    
    let mut files = HashMap::new();
    collect_lua_files_rec(dir, dir, &mut files)?;
    Ok(files)
}

fn collect_lua_files_rec(
    root: &PathBuf,
    current: &PathBuf,
    files: &mut std::collections::HashMap<String, Vec<u8>>,
) -> Result<()> {
    use std::fs;
    
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            collect_lua_files_rec(root, &path, files)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("lua") {
            let content = fs::read(&path)?;
            let rel_path = path.strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            files.insert(rel_path, content);
        }
    }
    Ok(())
}
