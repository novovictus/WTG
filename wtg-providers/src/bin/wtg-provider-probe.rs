use wtg_providers::amd_adl;

fn main() {
    let mut args = std::env::args().skip(1);
    let provider = match args.next() {
        Some(provider) => provider,
        None => usage_error("provider name is required"),
    };

    match provider.as_str() {
        "amd-adl" => std::process::exit(amd_adl::run_cli(args)),
        other => usage_error(&format!("unknown provider: {other}")),
    }
}

fn usage_error(message: &str) -> ! {
    eprintln!(
        "WTG provider probe usage error: {message}\nUsage: wtg-provider-probe amd-adl --once|--watch [--interval-ms <ms>]"
    );
    std::process::exit(2);
}
