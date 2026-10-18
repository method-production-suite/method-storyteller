//! Refuses the updater's rehearsal values (a local endpoint, a throwaway key, plain HTTP) in
//! any build but a rehearsal one, so a release can never carry them.

fn main() {
    let rehearsal = std::env::var_os("CARGO_FEATURE_REHEARSAL").is_some();
    for suffix in ["_ENDPOINT", "_PUBKEY", "_INSECURE"] {
        let name = format!("METHOD_STORYTELLER_UPDATE{suffix}");
        println!("cargo:rerun-if-env-changed={name}");
        if !rehearsal && std::env::var_os(&name).is_some() {
            println!("cargo:error={name} is set, but this is not a rehearsal build.");
            std::process::exit(1);
        }
    }
}
