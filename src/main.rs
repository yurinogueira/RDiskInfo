use rdiskinfo::config::{AppConfig, ThemeKind};
use rdiskinfo::export::format_full_report;
use rdiskinfo::i18n::{LanguageChoice, Strings};
use rdiskinfo::privilege::{BackendManager, is_root, run_privileged_helper};
use rdiskinfo::raw_ioctl::read_raw_smart_binary;
use rdiskinfo::ui::{RDiskInfoApp, build_app_icon, sync_user_desktop_integration};
use std::io::Write;
use std::process;
use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--privileged-helper") {
        let code = run_privileged_helper();
        process::exit(code);
    }

    if let Some(pos) = args.iter().position(|a| a == "--ascii-view") {
        let Some(dev) = args.get(pos + 1) else {
            eprintln!("Error: Missing device path after --ascii-view");
            process::exit(1);
        };
        if !is_root() {
            eprintln!("Error: --ascii-view requires root privileges!");
            process::exit(1);
        }
        match read_raw_smart_binary(dev) {
            Ok(bytes) => {
                let mut stdout = std::io::stdout();
                let _ = stdout.write_all(&bytes);
                let _ = stdout.flush();
                process::exit(0);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        }
    }

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return;
    }

    let mut cfg = AppConfig::load();
    let no_elevate = args.iter().any(|a| a == "--no-elevate");

    if let Some(pos) = args.iter().position(|a| a == "--lang")
        && let Some(code) = args.get(pos + 1)
    {
        cfg.language = match code.to_ascii_lowercase().as_str() {
            "auto" => LanguageChoice::Auto,
            "en" => LanguageChoice::En,
            "pt" | "pt-br" | "pt_br" => LanguageChoice::PtBr,
            "es" => LanguageChoice::Es,
            "fr" => LanguageChoice::Fr,
            "de" => LanguageChoice::De,
            "it" => LanguageChoice::It,
            "ja" => LanguageChoice::Ja,
            "zh" | "zh-cn" | "zh_cn" => LanguageChoice::ZhCn,
            _ => cfg.language,
        };
    }

    let mut theme_override = None;
    if let Some(pos) = args.iter().position(|a| a == "--theme")
        && let Some(name) = args.get(pos + 1)
    {
        theme_override = match name.to_ascii_lowercase().as_str() {
            "light" | "native" => Some(ThemeKind::NativeLight),
            "dark" => Some(ThemeKind::ModernDark),
            "blue" | "crystal" => Some(ThemeKind::CrystalBlue),
            "sakura" | "aoi" => Some(ThemeKind::SakuraAoi),
            "sakura-night" | "sakura_night" => Some(ThemeKind::SakuraNight),
            "cyber" | "midnight" => Some(ThemeKind::CyberMidnight),
            _ => None,
        };
    }

    if args.iter().any(|a| a == "--cli" || a == "--json") {
        let json_mode = args.iter().any(|a| a == "--json");
        let backend = BackendManager::new();
        let report = backend.scan_disks(!no_elevate, &cfg.eval_options());
        if json_mode {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string())
            );
        } else {
            let tr = Strings::for_language(cfg.language.resolve());
            println!("{}", format_full_report(&report.disks, &cfg, tr));
        }
        return;
    }

    sync_user_desktop_integration();

    let initial_width = if theme_override.unwrap_or(cfg.theme).has_stylized_artwork() {
        1080.0
    } else {
        920.0
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RDiskInfo")
            .with_app_id("rdiskinfo")
            .with_inner_size([initial_width, 680.0])
            .with_min_inner_size([720.0, 520.0])
            .with_icon(Arc::new(build_app_icon())),
        ..Default::default()
    };

    let auto_elevate = !no_elevate;
    if let Err(err) = eframe::run_native(
        "RDiskInfo",
        options,
        Box::new(move |cc| {
            Ok(Box::new(RDiskInfoApp::new(
                cc,
                cfg,
                auto_elevate,
                theme_override,
            )))
        }),
    ) {
        eprintln!("Failed to launch RDiskInfo GUI: {}", err);
        process::exit(1);
    }
}

fn print_help() {
    println!(
        "RDiskInfo v{} - S.M.A.R.T. & Disk Information Tool for Linux\n\n\
         USAGE:\n\
           rdiskinfo [OPTIONS]\n\n\
         OPTIONS:\n\
           --cli                 Print comprehensive disk report to stdout and exit\n\
           --json                Print full JSON scan report to stdout and exit\n\
           --no-elevate          Do not automatically prompt for pkexec elevation\n\
           --theme <NAME>        Theme: light | dark | blue | sakura | sakura-night | cyber\n\
           --lang <CODE>         Language: auto | en | pt-br | es | fr | de | it | ja | zh-cn\n\
           --ascii-view <DEV>    Print raw binary SMART/Identify data (requires root)\n\
           -h, --help            Show this help message",
        env!("CARGO_PKG_VERSION")
    );
}
